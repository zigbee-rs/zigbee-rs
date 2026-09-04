//! Implements the Zigbee Base Device Behavior (BDB) in `no-std` based on the
//! [Zigbee Base Device Behavior Specification Rev. 13].
//!
//! [Zigbee Base Device Behavior Specification Rev. 13]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
//!
//! This crate defines the standard commissioning procedures all devices must
//! support. It is a thin commissioning orchestrator: it does not own the
//! [`ZigbeeDevice`] or the [`Nlme`] — instead, callers pass the device to a
//! commissioning procedure. Everything takes `&self`, like the layers below,
//! so one instance can be shared; post-commissioning the application interacts
//! with the stack through the APSDE-SAP exposed on [`ZigbeeDevice`].
#![no_std]
#![allow(unused)]

#[cfg(test)]
extern crate std;

use core::future::Future;
use core::future::poll_fn;
use core::pin::pin;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::task::Poll;

use embedded_hal_async::delay::DelayNs;
use spin::Mutex;
use thiserror::Error;

pub mod types;

// BDB 5.1, table 1
const BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS: u8 = 10;
const BDBC_MIN_COMMISSIONING_TIME: u8 = 0xb4;
const BDBC_REC_SAME_NETWORK_RETRY_ATTEMPTS: u8 = 3;
const BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT: u8 = 5;

// bdbTCLinkKeyExchangeAttemptsMax (BDB 5.3.12)
const TC_LINK_KEY_EXCHANGE_ATTEMPTS_MAX: u8 = 3;
// a trust center reporting this stack revision or an earlier one predates the
// TC link key exchange (BDB 10.2.5 step 5)
const LEGACY_STACK_REVISION: u8 = 20;

// bdbcTcLinkKeyExchangeTimeout in milliseconds
const TC_LINK_KEY_EXCHANGE_TIMEOUT_MS: u32 = BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT as u32 * 1_000;

use types::BdbCommissioningStatus;
use types::CommissioningMode;
use zigbee_core::Config;
use zigbee_core::LogicalType;
use zigbee_core::aps::aib;
use zigbee_core::aps::frame::command::Command;
use zigbee_core::aps::frame::command::RequestKey;
use zigbee_core::aps::frame::command::VerifyKey;
use zigbee_core::nwk::nib;
use zigbee_core::nwk::nib::CapabilityInformation;
use zigbee_core::nwk::nib::Nib;
use zigbee_core::nwk::nlme::NetworkError;
use zigbee_core::nwk::nlme::Nlme;
use zigbee_core::nwk::nlme::management::NlmeJoinConfirm;
use zigbee_core::nwk::nlme::management::NlmeJoinRequest;
use zigbee_core::nwk::nlme::management::NlmeJoinStatus;
use zigbee_core::nwk::nlme::management::NlmeNetworkFormationRequest;
use zigbee_core::nwk::nlme::management::NlmePermitJoiningRequest;
use zigbee_core::nwk::nlme::management::RejoinNetwork;
use zigbee_core::security::primitives::HmacAes128Mmo;
use zigbee_core::zdo::ZigbeeDevice;
use zigbee_core::zdo::descriptor::NodeDescRsp;
use zigbee_mac::mlme::MacConfig;
use zigbee_mac::mlme::Mlme;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::sync::with_timeout;

/// Base Device Behavior (BDB) commissioning manager.
///
/// Orchestrates the standard commissioning procedures defined in the BDB
/// specification: initialization, network steering, network formation,
/// finding & binding, and touchlink. The application owns the
/// [`ZigbeeDevice`] and the [`Nlme`]; commissioning methods borrow the device
/// for the duration of the procedure.
///
/// Like the layers below, procedures take `&self` and keep their few
/// attributes (BDB 5.3) behind interior mutability, so one instance can be
/// shared. They are not meant to overlap, though: run one commissioning
/// procedure at a time.
pub struct BaseDeviceBehavior {
    config: Config,
    // bdbNodeIsOnANetwork (BDB 5.3.9)
    bdb_node_is_on_a_network: AtomicBool,
    // bdbCommissioningMode (BDB 5.3.5)
    bdb_commissioning_mode: Mutex<CommissioningMode>,
    // bdbCommissioningStatus (BDB 5.3.1)
    bdb_commissioning_status: Mutex<BdbCommissioningStatus>,
    // whether network steering exchanges the TC link key (BDB 8.2 step 12)
    tc_link_key_exchange_enabled: AtomicBool,
}

impl BaseDeviceBehavior {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            bdb_node_is_on_a_network: AtomicBool::new(false),
            bdb_commissioning_mode: Mutex::new(CommissioningMode::NetworkSteering),
            bdb_commissioning_status: Mutex::new(BdbCommissioningStatus::Success),
            tc_link_key_exchange_enabled: AtomicBool::new(true),
        }
    }

    /// Returns a reference to the global NIB singleton.
    pub fn nib(&self) -> &'static Nib {
        nib::get_ref()
    }

    /// bdbCommissioningStatus (BDB 5.3.1): how the last commissioning
    /// procedure ended, or `InProgress` while one runs.
    pub fn commissioning_status(&self) -> BdbCommissioningStatus {
        *self.bdb_commissioning_status.lock()
    }

    /// bdbCommissioningMode (BDB 5.3.5): the procedure commissioning runs.
    pub fn commissioning_mode(&self) -> CommissioningMode {
        *self.bdb_commissioning_mode.lock()
    }

    /// bdbNodeIsOnANetwork (BDB 5.3.9): whether the node is on a network.
    pub fn node_is_on_a_network(&self) -> bool {
        self.bdb_node_is_on_a_network.load(Ordering::Relaxed)
    }

    /// Whether network steering exchanges the initial Trust Center link key
    /// for a unique one (BDB 8.2 step 12); a failed exchange takes the node
    /// off the network again.
    ///
    /// Disable to stay on the default global TC link key with a trust center
    /// that answers neither Node_Desc_req nor REQUEST-KEY, at the cost of
    /// leaving the join unauthenticated.
    pub fn set_tc_link_key_exchange(&self, enabled: bool) {
        self.tc_link_key_exchange_enabled
            .store(enabled, Ordering::Relaxed);
    }

    fn set_commissioning_status(&self, status: BdbCommissioningStatus) {
        *self.bdb_commissioning_status.lock() = status;
    }

    /// Initialization procedure (BDB 7.1).
    ///
    /// Persistent state (NIB/AIB) must already be restored by the caller
    /// before constructing the [`Nlme`]/[`ZigbeeDevice`] — that is step 1.
    /// If the node is not on a network, or is not an end device, this
    /// returns `Ok(None)` and the caller should invoke
    /// [`Self::network_steering`].
    ///
    /// Step 4 delegates to [`ZigbeeDevice::rejoin`], so a Trust Center rejoin
    /// backs up the secure one. If both fail the caller is responsible for
    /// falling back to [`Self::network_steering`] after
    /// [`ZigbeeDevice::forget_network`]; keeping the parent link alive
    /// afterwards is [`ZigbeeDevice::link_maintenance`].
    pub async fn start_initialization_procedure<M: Mlme>(
        &self,
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
    ) -> Result<Option<NlmeJoinConfirm>, NetworkError> {
        let nib = self.nib();

        // BDB 7.1 step 2
        let on_a_network = *nib.network_address() != 0xffff;
        self.bdb_node_is_on_a_network
            .store(on_a_network, Ordering::Relaxed);
        if !on_a_network {
            return Ok(None);
        }

        // BDB 7.1 step 3: only an end device attempts an automatic rejoin
        // here; a router's channel-verification step (7.1 steps 6-7) is not
        // yet implemented
        if !self.is_end_device() {
            return Ok(None);
        }

        // BDB 7.1 step 4: rejoin on the current channel, so no scan channels.
        // Read the guard's value out before the `.await` — held across it,
        // it'd deadlock against rejoin_confirm's write of the same NIB field.
        log::debug!("[BDB] step 4: attempt NWK rejoin");
        let extended_panid = *nib.extended_panid();
        let confirm = device
            .rejoin(IeeeAddress(extended_panid), 0..0, 0)
            .await?;

        // BDB 7.1 step 5
        if confirm.status == NlmeJoinStatus::Success {
            log::debug!("[BDB] step 5: rejoin succeeded");
            self.set_commissioning_status(BdbCommissioningStatus::Success);
        } else {
            log::warn!("[BDB] step 5: rejoin failed ({:?})", confirm.status);
            self.set_commissioning_status(BdbCommissioningStatus::NoNetwork);
        }

        Ok(Some(confirm))
    }

    /// Network steering procedure for a node not on a network (BDB 8.2).
    ///
    /// Discovers and joins the network at `extended_pan_id`, then exchanges
    /// keys with the Trust Center.
    ///
    /// The receive loop task must be spawned before calling this: it idles
    /// until the join completes (`ZigbeeDevice::rx_loop`), then delivers the
    /// Trust Center's replies for the link key exchange.
    pub async fn network_steering<M: Mlme>(
        &self,
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
        extended_pan_id: IeeeAddress,
        channels: core::ops::Range<u8>,
        scan_duration: u8,
        capability_information: CapabilityInformation,
    ) -> Result<NlmeJoinConfirm, NetworkError> {
        log::debug!(
            "[BDB] start network steering, EPID={extended_pan_id:?}, channels={channels:?}"
        );
        self.set_commissioning_status(BdbCommissioningStatus::InProgress);

        // a node steering onto a network holds no valid link keys for it: any
        // left over from an earlier network would reject the Trust Center's
        // Transport-Key (4.4.10)
        device.reset_trust_center_link_keys();

        // BDB 8.2 step 1
        device
            .nlme()
            .network_discovery(channels, scan_duration)
            .await?;

        // BDB 8.2 step 5
        let request = NlmeJoinRequest {
            extended_pan_id,
            rejoin_network: RejoinNetwork::Association,
            capability_information,
            security_enabled: false,
            // an association join selects its parent from the neighbor table
            // filled by the discovery above
            scan_channels: 0..0,
            scan_duration: 0,
        };
        let confirm = device.nlme().join(request).await;
        if confirm.status != NlmeJoinStatus::Success {
            self.set_commissioning_status(BdbCommissioningStatus::NoNetwork);
            return Ok(confirm);
        }

        // BDB 8.2 step 9
        log::debug!("[BDB] step 9: poll for network key (transport-key)");
        device.poll_transport_key().await?;
        log::debug!("[BDB] step 9: network key installed");

        // 3.6.10.2: negotiate the end-device timeout with the parent
        let _ = device.nlme().negotiate_end_device_timeout().await;

        // BDB 8.2 step 11
        log::debug!("[BDB] step 11: broadcast Device_annce");
        device.announce_self().await?;

        // BDB 8.2 step 12, BDB 10.2.5
        if self.tc_link_key_exchange_enabled.load(Ordering::Relaxed) {
            log::debug!("[BDB] step 12: TC link key exchange");
            // BDB 8.2 step 12: staying on the network with an unexchanged key
            // is not permitted — the node leaves and resets its network state
            if let Err(e) = self.tc_link_key_exchange(device, delay).await {
                log::warn!("[BDB] step 12: TC link key exchange failed ({e:?}), leaving network");
                self.set_commissioning_status(BdbCommissioningStatus::TclkExFailure);
                self.bdb_node_is_on_a_network
                    .store(false, Ordering::Relaxed);
                let status = device.leave_network(false).await;
                log::info!("[BDB] left the network after TCLK failure: {status:?}");
                return Err(e);
            }
            log::debug!("[BDB] step 12: TC link key exchange complete");
        } else {
            log::debug!("[BDB] step 12: TC link key exchange disabled, skipping");
        }

        self.bdb_node_is_on_a_network.store(true, Ordering::Relaxed);
        self.set_commissioning_status(BdbCommissioningStatus::Success);
        Ok(confirm)
    }

    // TC link key exchange procedure (BDB 10.2.5): REQUEST-KEY ->
    // TRANSPORT-KEY -> VERIFY-KEY -> CONFIRM-KEY; the receive loop consumes
    // the Trust Center's replies, this only drives the timing
    async fn tc_link_key_exchange<M: Mlme>(
        &self,
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
    ) -> Result<(), NetworkError> {
        // arm the stack: only now are Transport-Key/Confirm-Key honored, so
        // replayed frames outside the exchange cannot downgrade the key
        device.begin_tc_link_key_exchange();
        let result = self.run_tc_link_key_exchange(device, delay).await;
        device.end_tc_link_key_exchange();
        result
    }

    async fn run_tc_link_key_exchange<M: Mlme>(
        &self,
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
    ) -> Result<(), NetworkError> {
        let tc_short = ShortAddress(0x0000);
        let aib = aib::get_ref();
        let tc_ieee = *aib.trust_center_address();

        log::debug!("[BDB] start TC link key exchange, TC={tc_ieee:?}");

        // BDB 10.2.5 steps 2-5: a trust center on r20 or earlier knows no key
        // exchange, and asking it for a key would stall the commissioning
        let Some(node_desc) = self
            .request_trust_center_node_desc(device, delay, tc_short)
            .await
        else {
            log::warn!("[BDB] TC link key exchange failed: no Node_Desc_rsp");
            self.set_commissioning_status(BdbCommissioningStatus::TclkExFailure);
            return Err(NetworkError::NoTransportKey);
        };
        if node_desc
            .stack_revision()
            .is_some_and(|revision| revision <= LEGACY_STACK_REVISION)
        {
            log::info!(
                "[BDB] trust center reports stack revision {:?}, keeping the default TC link key",
                node_desc.stack_revision()
            );
            return Ok(());
        }

        // BDB 10.2.5 steps 6-9: the receive loop installs the transported
        // key as unverified and signals reception
        let mut attempts = 0u8;
        loop {
            device
                .send_aps_command(
                    tc_short,
                    tc_ieee,
                    Command::RequestKey(RequestKey::TrustCenterLinkKey),
                    true,
                )
                .await?;
            attempts += 1;

            match with_timeout(
                device.wait_tc_link_key_received(),
                delay.delay_ms(TC_LINK_KEY_EXCHANGE_TIMEOUT_MS),
            )
            .await
            {
                Some(true) => {
                    log::debug!("[BDB] received new TC link key");
                    break;
                }
                // step 9: the transported key was rejected — it is the one we
                // already hold, which ends the procedure rather than retrying
                Some(false) => {
                    log::warn!("[BDB] TC link key exchange failed: unusable TRANSPORT-KEY");
                    self.set_commissioning_status(BdbCommissioningStatus::TclkExFailure);
                    return Err(NetworkError::NoTransportKey);
                }
                None if attempts >= TC_LINK_KEY_EXCHANGE_ATTEMPTS_MAX => {
                    log::warn!("[BDB] TC link key exchange failed: no TRANSPORT-KEY");
                    self.set_commissioning_status(BdbCommissioningStatus::TclkExFailure);
                    return Err(NetworkError::NoTransportKey);
                }
                None => continue,
            }
        }

        // BDB 10.2.5 step 9: the unverified key is now in the AIB
        let new_key = aib
            .device_key_pair_set()
            .iter()
            .find(|k| k.device_address == tc_ieee)
            .map(|k| k.link_key)
            .ok_or(NetworkError::NoTransportKey)?;

        // BDB 10.2.5 steps 10-13, 4.4.10.7.4
        let hash = HmacAes128Mmo::hmac(new_key.as_slice(), &[0x03]).map_err(|_| {
            NetworkError::SecurityError(zigbee_core::security::SecurityError::Unspecified)
        })?;
        // 4.4.10.7.3
        let device_addr = *nib::get_ref().ieee_address();

        let mut attempts = 0u8;
        loop {
            device
                .send_aps_command(
                    tc_short,
                    tc_ieee,
                    Command::VerifyKey(VerifyKey {
                        key_type: 0x04,
                        source_address: device_addr,
                        hash: ByteArray(hash),
                    }),
                    false,
                )
                .await?;
            attempts += 1;

            match with_timeout(
                device.wait_tc_link_key_verified(),
                delay.delay_ms(TC_LINK_KEY_EXCHANGE_TIMEOUT_MS),
            )
            .await
            {
                Some(0x00) => {
                    log::debug!("[BDB] TC link key verified successfully");
                    return Ok(());
                }
                // rejected verification: retry immediately instead of
                // burning the full timeout
                Some(status) if attempts < BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS => {
                    log::warn!("[BDB] CONFIRM-KEY status {status:#04x}, retrying");
                }
                None if attempts < BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS => continue,
                _ => {
                    log::warn!("[BDB] TC link key exchange failed: no CONFIRM-KEY");
                    self.set_commissioning_status(BdbCommissioningStatus::TclkExFailure);
                    return Err(NetworkError::NoTransportKey);
                }
            }
        }
    }

    // BDB 10.2.5 steps 3-4: read the trust center's node descriptor, retrying
    // until bdbTCLinkKeyExchangeAttemptsMax attempts are spent
    async fn request_trust_center_node_desc<M: Mlme>(
        &self,
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
        tc_short: ShortAddress,
    ) -> Option<NodeDescRsp> {
        for _ in 0..TC_LINK_KEY_EXCHANGE_ATTEMPTS_MAX {
            if let Err(e) = device.node_desc_req(tc_short).await {
                log::warn!("[BDB] Node_Desc_req to the trust center failed ({e:?})");
                delay.delay_ms(TC_LINK_KEY_EXCHANGE_TIMEOUT_MS).await;
                continue;
            }
            if let Some(response) = with_timeout(
                device.wait_node_desc_rsp(),
                delay.delay_ms(TC_LINK_KEY_EXCHANGE_TIMEOUT_MS),
            )
            .await
            {
                return Some(response);
            }
        }
        None
    }

    fn is_end_device(&self) -> bool {
        self.config.device_type == LogicalType::EndDevice
    }

    fn is_router(&self) -> bool {
        self.config.device_type == LogicalType::Router
    }
}

#[derive(Debug, Error)]
pub enum BdbError {
    #[error("network error")]
    NetworkError(#[from] NetworkError),

    #[error("no open network discovered to join")]
    NoNetwork,
}

#[cfg(test)]
mod tests {
    use core::future::Future;
    use core::sync::atomic::AtomicBool;
    use core::sync::atomic::Ordering;

    use zigbee_core::nwk::nib;
    use zigbee_mac::Address;
    use zigbee_mac::mlme::AssociationResponse;
    use zigbee_mac::mlme::MacError;
    use zigbee_mac::mlme::ScanResult;
    use zigbee_mac::mlme::ScanType;

    use super::*;

    #[allow(clippy::panic)]
    fn block_on<F: Future>(f: F) -> F::Output {
        use core::pin::pin;
        use core::task::Context;
        use core::task::Poll;
        use core::task::RawWaker;
        use core::task::RawWakerVTable;
        use core::task::Waker;

        fn noop(_: *const ()) {}
        fn clone(p: *const ()) -> RawWaker {
            RawWaker::new(p, &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);

        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut f = pin!(f);

        match f.as_mut().poll(&mut cx) {
            Poll::Ready(val) => val,
            Poll::Pending => panic!("block_on: future returned Pending"),
        }
    }

    mockall::mock! {
        Mlme {}
        impl Mlme for Mlme {
            fn ieee_address(&self) -> IeeeAddress;
            async fn configure(&self, config: MacConfig);
            async fn scan_network(
                &self,
                ty: ScanType,
                channels: core::ops::Range<u8>,
                duration: u8,
            ) -> Result<ScanResult, MacError>;
            async fn associate(
                &self,
                channel: u8,
                dest: Address,
                capabilities: zigbee_mac::CapabilityInformation,
            ) -> Result<AssociationResponse, MacError>;
            async fn poll_data(
                &self,
                coord_address: Address,
                buf: &mut [u8],
            ) -> Result<(usize, u8), MacError>;
            async fn receive(
                &self,
                buf: &mut [u8],
            ) -> Result<(usize, u8), MacError>;
            async fn transmit_data(
                &self,
                dest: Address,
                payload: &[u8],
            ) -> Result<(), MacError>;
        }
    }

    // returns immediately, so block_on never sees Pending
    struct NoopDelay;

    impl DelayNs for NoopDelay {
        async fn delay_ns(&mut self, _ns: u32) {}
    }

    fn make_device(device_type: LogicalType) -> ZigbeeDevice<MockMlme> {
        let mut mac = MockMlme::new();
        mac.expect_ieee_address()
            .return_const(IeeeAddress(0xa4c1_0000_0000_0001));
        let nlme = Nlme::new(mac);
        let config = Config {
            device_type,
            ..Config::default()
        };
        ZigbeeDevice::new(config, nlme)
    }

    // NIB is a process-wide singleton with a non-cfg(test) `init()` (no
    // `try_init`/`reset` outside the `zigbee-core` crate itself), so every case
    // that needs it lives in one #[test] to keep initialization order
    // deterministic across the crate's test binary.
    #[test]
    fn start_initialization_procedure_early_returns() {
        nib::init();
        zigbee_core::aps::aib::init();

        let device = make_device(LogicalType::EndDevice);
        let bdb = BaseDeviceBehavior::new(Config {
            device_type: LogicalType::EndDevice,
            ..Config::default()
        });
        let result = block_on(bdb.start_initialization_procedure(&device, &mut NoopDelay));
        assert!(matches!(result, Ok(None)));
        assert!(!bdb.node_is_on_a_network());

        nib::get_ref().update_network_address(|value| *value = 0x1234);
        let router_device = make_device(LogicalType::Router);
        let router_bdb = BaseDeviceBehavior::new(Config {
            device_type: LogicalType::Router,
            ..Config::default()
        });
        let result =
            block_on(router_bdb.start_initialization_procedure(&router_device, &mut NoopDelay));
        assert!(matches!(result, Ok(None)));
        assert!(router_bdb.node_is_on_a_network());
    }
}
