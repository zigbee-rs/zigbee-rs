//! The runtime object an application spawns its tasks against.

use core::future::Future;
use core::pin::pin;
use core::task::Poll;
use core::task::ready;

use embedded_hal_async::delay::DelayNs;
use zigbee_base_device_behavior::BaseDeviceBehavior;
use zigbee_core::nwk::frame::command::network_status::NetworkStatusCode;
use zigbee_core::nwk::nib;
use zigbee_core::nwk::nlme::KeepaliveMethod;
use zigbee_core::nwk::nlme::NetworkError;
use zigbee_core::nwk::nlme::Nlme;
use zigbee_core::nwk::nlme::management::NlmeJoinConfirm;
use zigbee_core::nwk::nlme::management::NlmeJoinStatus;
use zigbee_core::storage::NoStorage;
use zigbee_core::storage::StorageDriver;
use zigbee_core::zdo::ClusterRequestHandler;
use zigbee_core::zdo::ZigbeeDevice;
use zigbee_mac::mlme::Mlme;
use zigbee_types::ShortAddress;

use crate::config::StackConfig;

/// How [`Stack::commission`] got the device onto the network.
#[derive(Debug)]
pub enum Commissioned {
    /// The device rejoined the network it was already on (BDB 7.1).
    Rejoined(NlmeJoinConfirm),
    /// The device joined and was commissioned from scratch (BDB 8.2).
    Steered(NlmeJoinConfirm),
    /// Neither rejoining nor steering got the device onto a network.
    Failed(NlmeJoinStatus),
}

/// Why [`Stack::run`] returned: the device is off the network and could not
/// get back on by itself.
#[derive(Debug)]
pub enum StackStopped {
    /// Commissioning could not put the device onto a network (BDB 8.2).
    Commissioning(NlmeJoinStatus),
    /// The network was lost and no rejoin got the device back (3.6.10.10).
    Rejoin(NlmeJoinStatus),
    /// A layer below reported an error.
    Error(NetworkError),
}

/// The running stack: the device, its configuration, the cluster handler
/// answering inbound requests, and where state is persisted.
///
/// Build it once, keep it alive for the program (a `StaticCell` does), and
/// spawn [`Self::run`] in a task; every other task borrows the same instance
/// to talk to the network.
pub struct Stack<'d, M, H, S = NoStorage> {
    device: ZigbeeDevice<M>,
    config: StackConfig<'d>,
    handler: H,
    storage: S,
    bdb: BaseDeviceBehavior,
}

impl<'d, M, H, S> Stack<'d, M, H, S>
where
    M: Mlme,
    H: ClusterRequestHandler,
    S: StorageDriver,
{
    /// Build the stack on top of a MAC.
    ///
    /// The information bases must already be restored (`zigbee_core::storage`),
    /// because the NWK layer reads them here.
    pub fn new(mac: M, config: StackConfig<'d>, handler: H, storage: S) -> Self {
        let device = ZigbeeDevice::new(config.zdo(), Nlme::new(mac));
        let bdb = BaseDeviceBehavior::new(config.zdo());
        bdb.set_tc_link_key_exchange(config.device().tc_link_key_exchange);
        Self {
            device,
            config,
            handler,
            storage,
            bdb,
        }
    }

    /// Run the stack: get onto the network, then stay on it.
    ///
    /// Commissions the device first (resume, rejoin, or steer onto the
    /// configured network), then keeps the parent link alive, all while
    /// receiving and dispatching inbound frames and persisting
    /// information-base changes. The application only has to spawn this.
    ///
    /// Returns when the device is off the network and cannot get back on, so
    /// the caller can decide between re-commissioning and a reset; use
    /// [`Self::wait_until_joined`] to learn when the device is on the network
    /// instead of waiting for this.
    pub async fn run<D: DelayNs + Clone>(&self, delay: D) -> StackStopped {
        self.resume().await;
        let mut rx_delay = delay.clone();
        let mut main_delay = delay;

        // the receive loop has to run during commissioning: it idles until the
        // join completes, then delivers the Trust Center's replies
        let receive = async { self.rx_loop(&mut rx_delay).await };
        let persist = self.storage.run();
        let network = async {
            match self.commission(&mut main_delay).await {
                Ok(Commissioned::Rejoined(_) | Commissioned::Steered(_)) => (),
                Ok(Commissioned::Failed(status)) => {
                    return StackStopped::Commissioning(status);
                }
                Err(e) => return StackStopped::Error(e),
            }

            match self.link_maintenance(&mut main_delay).await {
                Ok(confirm) => StackStopped::Rejoin(confirm.status),
                Err(e) => StackStopped::Error(e),
            }
        };

        drive(network, receive, persist).await
    }
}

/// A configured extended PAN id of zero steers onto any network (BDB 8.2).
const ANY_EXTENDED_PAN_ID: u64 = 0;

/// Whether a remembered network satisfies the configured extended PAN id.
///
/// A configured wildcard accepts whatever network the device is on. Otherwise
/// the identifiers have to agree — including the case where the remembered
/// network carries no identifier at all, which cannot be shown to be the
/// configured one and so does not match.
const fn epid_matches(configured: u64, remembered: u64) -> bool {
    configured == ANY_EXTENDED_PAN_ID || configured == remembered
}

// polls `main` to completion while the background futures keep running
async fn drive<T>(
    main: impl Future<Output = T>,
    background: impl Future<Output = ()>,
    persist: impl Future<Output = ()>,
) -> T {
    let mut main = pin!(main);
    let mut background = pin!(background);
    let mut persist = pin!(persist);
    core::future::poll_fn(move |cx| {
        let _ = background.as_mut().poll(cx);
        let _ = persist.as_mut().poll(cx);
        Poll::Ready(ready!(main.as_mut().poll(cx)))
    })
    .await
}

impl<'d, M, H, S> Stack<'d, M, H, S>
where
    M: Mlme,
    H: ClusterRequestHandler,
    S: StorageDriver,
{
    /// The device this stack drives, for everything the application sends and
    /// asks itself (APSDE-SAP, binding, leaving).
    pub fn device(&self) -> &ZigbeeDevice<M> {
        &self.device
    }

    /// The configuration this stack runs with.
    pub fn config(&self) -> &StackConfig<'d> {
        &self.config
    }

    /// The BDB commissioning manager, for procedures the stack does not run
    /// itself — network formation, finding & binding, touchlink — and for the
    /// commissioning status (BDB 5.3.1).
    ///
    /// Commissioning procedures are not meant to overlap: do not start one
    /// while [`Self::run`] is commissioning.
    pub fn bdb(&self) -> &BaseDeviceBehavior {
        &self.bdb
    }

    /// Program the radio from the restored information base (3.6.8).
    ///
    /// A device reset while joined keeps its addresses but comes up with an
    /// unconfigured radio; this applies the PAN id, short address and channel
    /// so it can resume where it left off instead of re-commissioning.
    /// [`Self::run`] and [`Self::commission`] do this for you.
    pub async fn resume(&self) -> bool {
        let resuming = self.device.nlme().resume(self.config.channel()).await;
        if resuming {
            log::info!(
                "[APP] resuming on network: addr={:#06x}",
                *nib::get_ref().network_address()
            );
        }
        resuming
    }

    /// Wait until the device is on the network with the network key
    /// installed, so the application can start talking to it.
    pub async fn wait_until_joined(&self) {
        self.device.wait_until_joined().await;
    }

    /// Forget the joined network and persist that, so the next boot
    /// re-commissions instead of resuming.
    pub async fn forget_network(&self) {
        self.device.forget_network();
        self.storage.flush().await;
    }

    /// Bring the device onto its network at startup.
    ///
    /// Runs the BDB initialization procedure first (7.1): a device that is
    /// still on a network rejoins it, keeping its keys. If that fails, the
    /// remembered network is forgotten and network steering commissions the
    /// device from scratch (BDB 8.2).
    ///
    /// A remembered network whose extended PAN id is not the configured one is
    /// forgotten before the rejoin, so changing the configured network takes
    /// effect on the next boot rather than being masked by the persisted
    /// information base. [`Self::run`] does this for you; call it
    /// directly only when driving the stack's loops as separate tasks, and make
    /// sure [`Self::rx_loop`] is already running — it delivers the Trust
    /// Center's replies.
    pub async fn commission(&self, delay: &mut impl DelayNs) -> Result<Commissioned, NetworkError> {
        let on_a_network = self.resume().await;
        let device = &self.device;
        let config = &self.config;
        let bdb = &self.bdb;

        // the information base outlives a firmware update, so a device that
        // joined under an earlier configuration would keep rejoining that
        // network and silently ignore the new one
        let configured_epid = config.network().extended_pan_id.0;
        let remembered_epid = *nib::get_ref().extended_panid();
        if on_a_network && !epid_matches(configured_epid, remembered_epid) {
            log::warn!(
                "[APP] joined network epid={remembered_epid:#018x} is not the configured \
                 {configured_epid:#018x}, forgetting it and commissioning again"
            );
            device.forget_network();
        }

        match bdb.start_initialization_procedure(device, delay).await? {
            Some(confirm) if confirm.status == NlmeJoinStatus::Success => {
                return Ok(Commissioned::Rejoined(confirm));
            }
            // on a network but unable to rejoin: the stale address and keys would
            // make NLME-JOIN refuse to re-associate, so drop them first
            Some(confirm) => {
                log::warn!(
                    "[APP] rejoin failed ({:?}), forgetting network",
                    confirm.status
                );
                device.forget_network();
            }
            // not on a network at all
            None => (),
        }

        let network = config.network();
        let confirm = bdb
            .network_steering(
                device,
                delay,
                network.extended_pan_id,
                network.channels.clone(),
                network.scan_duration,
                config.device().capability_information,
            )
            .await?;

        if confirm.status == NlmeJoinStatus::Success {
            Ok(Commissioned::Steered(confirm))
        } else {
            Ok(Commissioned::Failed(confirm.status))
        }
    }

    /// Run the receive/dispatch loop forever.
    ///
    /// Waits for the join to complete before touching the radio, so it can be
    /// started ahead of commissioning. The receive strategy follows the joined
    /// capability (`nwkCapabilityInformation`, 3.4.1.3.1.1): a device with
    /// `rxOnWhenIdle = TRUE` listens for frames directly, while a sleepy end
    /// device polls its parent every `poll_interval_ms`.
    ///
    /// A leave takes the device off the network and parks the loop until it is
    /// back on. [`Self::run`] drives this for you.
    pub async fn rx_loop(&self, delay: &mut impl DelayNs) -> ! {
        let device = &self.device;
        let handler = &self.handler;
        let cfg = self.config.descriptors();
        // a restored network address means the device resumed on a network
        // without a fresh key exchange — release the gate immediately
        if *nib::get_ref().network_address() != ShortAddress::default().0 {
            device.mark_rejoined();
        }
        device.wait_until_joined().await;

        if nib::get_ref()
            .capability_information()
            .receiver_on_when_idle()
        {
            log::trace!("[app]: start rx loop (rx on)");
            loop {
                // dispatching a leave closes the gate again, parking the loop
                // until the device has been rejoined or re-commissioned
                device.wait_until_joined().await;
                if let Err(e) = device.receive_and_dispatch(cfg, handler).await {
                    log::debug!("[APP] rx dispatch error: {e:?}");
                }
            }
        }

        log::trace!("[app]: start rx loop (rx off, polling)");
        loop {
            device.wait_until_joined().await;
            match device.poll_and_dispatch(cfg, handler).await {
                // an acknowledged data poll shows the parent still holds this
                // device in its neighbor table (3.6.10.4)
                Ok(()) => {
                    if device.nlme().keepalive_method() == KeepaliveMethod::MacDataPoll {
                        device.nlme().refresh_parent_timeout();
                    }
                }
                Err(e) => log::debug!("[APP] rx dispatch error: {e:?}"),
            }
            delay.delay_ms(self.config.timing().poll_interval_ms).await;
        }
    }

    /// Keep the parent link alive and rejoin whenever it is lost.
    ///
    /// Sends the periodic end-device keepalive (3.6.10.3) — for sleepy and
    /// `rxOnWhenIdle` devices alike — and reacts to everything that takes this
    /// device off the network: a failed keepalive, an expired local end-device
    /// timeout (3.6.10.6), a failed parent link reported while sending
    /// (3.6.3.7.1), and a leave asking the device to come back (3.6.1.10.4).
    /// Recovery is [`ZigbeeDevice::rejoin`], which keeps the keys.
    ///
    /// Returns only when the device is off the network for good.
    /// [`Self::run`] drives this for you.
    pub async fn link_maintenance(
        &self,
        delay: &mut impl DelayNs,
    ) -> Result<NlmeJoinConfirm, NetworkError> {
        let device = &self.device;
        let config = &self.config;
        loop {
            let interval = device
                .nlme()
                .keepalive_interval_ms()
                .unwrap_or(config.timing().default_keepalive_interval_ms);
            delay.delay_ms(interval).await;

            // the error already covers a drained local end-device timeout
            let keepalive_failed = self.keepalive().await.is_err();
            let parent_link_failed = matches!(
                device.nlme().take_nwk_status_indication(),
                Some(indication) if indication.status == NetworkStatusCode::ParentLinkFailure
            );
            let rejoin_requested = device.take_rejoin_request();

            if !(keepalive_failed || parent_link_failed || rejoin_requested) {
                continue;
            }

            log::info!(
                "[APP] parent link lost (keepalive={keepalive_failed}, status={parent_link_failed}, leave={rejoin_requested}), rejoining"
            );
            let network = config.network();
            let confirm = device
                .rejoin(
                    network.extended_pan_id,
                    network.channels.clone(),
                    network.scan_duration,
                )
                .await?;
            if confirm.status != NlmeJoinStatus::Success {
                return Ok(confirm);
            }
        }
    }

    /// Send one keepalive to the parent (3.6.10.3).
    ///
    /// Every end device — sleepy or `rxOnWhenIdle` — refreshes its entry in
    /// the parent's neighbor table this way, using the method the parent
    /// announced: a MAC data poll, whose retrieved frame is dispatched like in
    /// [`Self::rx_loop`], or an End Device Timeout Request. Without a
    /// negotiated timeout there is nothing to send and it succeeds silently.
    ///
    /// `Err(NetworkError::ParentLinkFailure)` means the link is gone — the
    /// parent rejected or ignored a timeout request, or enough keepalives went
    /// unanswered to run out the local end-device timeout (3.6.10.6).
    pub async fn keepalive(&self) -> Result<(), NetworkError> {
        let device = &self.device;
        let handler = &self.handler;
        let nlme = device.nlme();
        let cfg = self.config.descriptors();
        let result = match nlme.keepalive_method() {
            // the data poll itself is the keepalive, so dispatch what it brings
            // back instead of dropping it
            KeepaliveMethod::MacDataPoll => {
                let result = device.poll_and_dispatch(cfg, handler).await;
                if result.is_ok() {
                    nlme.refresh_parent_timeout();
                }
                result
            }
            KeepaliveMethod::TimeoutRequest | KeepaliveMethod::None => nlme.send_keepalive().await,
        };
        let Err(e) = result else {
            return Ok(());
        };

        // 3.6.10.6: an unanswered keepalive drains the local timeout, so a
        // single missed one does not yet count as a lost parent
        log::warn!("[APP] keepalive failed: {e:?}");
        let interval = nlme.keepalive_interval_ms().unwrap_or(0);
        if nlme.tick_parent_timeout(interval) {
            return Err(NetworkError::ParentLinkFailure);
        }
        Err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETWORK: u64 = 0x00124b002a9a7166;

    // BDB 8.2: a wildcard takes whatever network the device is on
    #[test]
    fn a_wildcard_configuration_accepts_any_remembered_network() {
        assert!(epid_matches(ANY_EXTENDED_PAN_ID, NETWORK));
        assert!(epid_matches(ANY_EXTENDED_PAN_ID, ANY_EXTENDED_PAN_ID));
    }

    #[test]
    fn a_configured_network_accepts_only_itself() {
        assert!(epid_matches(NETWORK, NETWORK));
        assert!(!epid_matches(NETWORK, 0x00124b002a9a7167));
    }

    // a device joined before the identifier was recorded cannot be shown to be
    // on the configured network, so it is re-commissioned rather than resumed
    #[test]
    fn a_remembered_network_without_an_identifier_does_not_match() {
        assert!(!epid_matches(NETWORK, ANY_EXTENDED_PAN_ID));
    }
}
