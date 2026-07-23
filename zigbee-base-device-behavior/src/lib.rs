//! Implements the Zigbee Base Device Behavior (BDB) in `no-std` based on the
//! [ZigBee Base Device Behavior Specification Rev. 13].
//!
//! [ZigBee Base Device Behavior Specification Rev. 13]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
//!
//! This crate defines the standard commissioning procedures all devices must
//! support. It is a thin commissioning orchestrator: it does not own the
//! [`ZigbeeDevice`] or the [`Nlme`] — instead, callers pass them by mutable
//! reference for the duration of a commissioning procedure. Post-commissioning
//! the application interacts with the stack through the APSDE-SAP exposed on
//! [`ZigbeeDevice`].
#![no_std]
#![allow(unused)]

#[cfg(test)]
extern crate std;

use core::future::Future;
use core::future::poll_fn;
use core::pin::pin;
use core::task::Poll;

use embedded_hal_async::delay::DelayNs;
use thiserror::Error;

pub mod types;

// BDB 5.1 | Table 1
const BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS: u8 = 10;
const BDBC_MIN_COMMISSIONING_TIME: u8 = 0xb4;
const BDBC_REC_SAME_NETWORK_RETRY_ATTEMPTS: u8 = 3;
const BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT: u8 = 5;

/// bdbcTcLinkKeyExchangeTimeout in milliseconds
const TC_LINK_KEY_EXCHANGE_TIMEOUT_MS: u32 = BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT as u32 * 1_000;

use types::BdbCommissioningStatus;
use types::CommissioningMode;
use zigbee::Config;
use zigbee::LogicalType;
use zigbee::aps::aib;
use zigbee::aps::frame::command::Command;
use zigbee::aps::frame::command::RequestKey;
use zigbee::aps::frame::command::VerifyKey;
use zigbee::nwk::nib;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nib::Nib;
use zigbee::nwk::nlme::NetworkError;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::management::NlmeJoinConfirm;
use zigbee::nwk::nlme::management::NlmeJoinRequest;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee::nwk::nlme::management::NlmeNetworkFormationRequest;
use zigbee::nwk::nlme::management::NlmePermitJoiningRequest;
use zigbee::nwk::nlme::management::RejoinNetwork;
use zigbee::security::primitives::HmacAes128Mmo;
use zigbee::zdo::ZigbeeDevice;
use zigbee::zdp::device_annce::DeviceAnnce;
use zigbee_mac::mlme::Mlme;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

/// Base Device Behavior (BDB) commissioning manager.
///
/// Orchestrates the standard commissioning procedures defined in the BDB
/// specification: initialization, network steering, network formation,
/// finding & binding, and touchlink. The application owns the
/// [`ZigbeeDevice`] and the [`Nlme`]; commissioning methods borrow them
/// for the duration of the procedure.
pub struct BaseDeviceBehavior {
    config: Config,
    bdb_node_is_on_a_network: bool,
    bdb_commissioning_mode: CommissioningMode,
    bdb_commissioning_status: BdbCommissioningStatus,
    /// Whether network steering performs the TC link key exchange (BDB §8.2
    /// step 12). Disable for trust centers that never answer REQUEST-KEY to
    /// keep the default global TC link key and skip the retry stalls.
    pub tc_link_key_exchange_enabled: bool,
}

impl BaseDeviceBehavior {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            bdb_node_is_on_a_network: false,
            bdb_commissioning_mode: CommissioningMode::NetworkSteering,
            bdb_commissioning_status: BdbCommissioningStatus::Success,
            tc_link_key_exchange_enabled: true,
        }
    }

    /// Returns a reference to the global NIB singleton.
    pub fn nib(&self) -> &'static Nib {
        nib::get_ref()
    }

    /// Initialization procedure (BDB §7.1).
    ///
    /// Persistent state (NIB/AIB) must already be restored by the caller
    /// before constructing the [`Nlme`]/[`ZigbeeDevice`] — that is step 1.
    /// If the node is not on a network, or is not an end device, this
    /// returns `Ok(None)` and the caller should invoke
    /// [`Self::network_steering`].
    ///
    /// Otherwise it attempts an NWK rejoin (§3.2.2.13.3) against the
    /// remembered network and returns the resulting confirm. A rejoin
    /// failure is not retried here — per §7.1 step 5 that is the caller's
    /// responsibility (e.g. falling back to [`Self::network_steering`] after
    /// [`ZigbeeDevice::forget_network`]).
    pub async fn start_initialization_procedure<M: Mlme>(
        &mut self,
        device: &ZigbeeDevice<M>,
    ) -> Result<Option<NlmeJoinConfirm>, NetworkError> {
        let nib = self.nib();

        // §7.1 step 2
        self.bdb_node_is_on_a_network = *nib.network_address() != 0xffff;
        if !self.bdb_node_is_on_a_network {
            return Ok(None);
        }

        // §7.1 step 3: only an end device attempts an automatic rejoin here;
        // a router's channel-verification step (§7.1 steps 6-7) is not yet
        // implemented.
        if !self.is_end_device() {
            return Ok(None);
        }

        // §7.1 step 4
        log::debug!("[BDB] step 4: attempt NWK rejoin");
        let capability_information = *nib.capability_information();
        let request = NlmeJoinRequest {
            extended_pan_id: IeeeAddress(*nib.extended_panid()),
            rejoin_network: RejoinNetwork::NwkRejoin,
            capability_information,
            security_enabled: true,
        };
        let confirm = device.nlme().join(request).await;

        // §7.1 step 5
        if confirm.status == NlmeJoinStatus::Success {
            log::debug!("[BDB] step 5: rejoin succeeded, broadcasting Device_annce");
            Self::device_annce(device, capability_information).await?;
            self.bdb_commissioning_status = BdbCommissioningStatus::Success;
        } else {
            log::warn!("[BDB] step 5: rejoin failed ({:?})", confirm.status);
            self.bdb_commissioning_status = BdbCommissioningStatus::NoNetwork;
        }

        Ok(Some(confirm))
    }

    /// Network steering procedure for a node NOT on a network
    /// (BDB §8.2).
    ///
    /// Performs NLME-NETWORK-DISCOVERY on the given channels, then
    /// NLME-JOIN for the specified extended PAN ID, the APS transport key
    /// exchange to obtain the network key from the Trust Center, and
    /// finally the TC link key exchange.
    ///
    /// The receive loop task must be spawned before calling this: it idles
    /// until the join completes (`ZigbeeDevice::rx_loop`), then delivers the
    /// Trust Center's replies for the link key exchange.
    pub async fn network_steering<M: Mlme>(
        &mut self,
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
        self.bdb_commissioning_status = BdbCommissioningStatus::InProgress;

        // §8.2 step 1
        device
            .nlme()
            .network_discovery(channels, scan_duration)
            .await?;

        // §8.2 step 5
        let request = NlmeJoinRequest {
            extended_pan_id,
            rejoin_network: RejoinNetwork::Association,
            capability_information,
            security_enabled: false,
        };
        let confirm = device.nlme().join(request).await;
        if confirm.status != NlmeJoinStatus::Success {
            self.bdb_commissioning_status = BdbCommissioningStatus::NoNetwork;
            return Ok(confirm);
        }

        // §8.2 step 9
        log::debug!("[BDB] step 9: poll for network key (transport-key)");
        device.poll_transport_key().await?;
        log::debug!("[BDB] step 9: network key installed");

        // §3.6.10.2: negotiate the end-device timeout with the parent
        Self::negotiate_end_device_timeout(device, delay).await;

        // §8.2 step 11
        log::debug!("[BDB] step 11: broadcast Device_annce");
        Self::device_annce(device, capability_information).await?;

        // §8.2 step 12, §10.2.5
        if self.tc_link_key_exchange_enabled {
            log::debug!("[BDB] step 12: TC link key exchange");
            // non-fatal: a TC allowing legacy devices may never answer the
            // REQUEST-KEY, keep the default global TC link key then
            match self.tc_link_key_exchange(device, delay).await {
                Ok(()) => log::debug!("[BDB] step 12: TC link key exchange complete"),
                Err(e) => log::warn!(
                    "[BDB] step 12: TC link key exchange failed ({e:?}), continuing with default TC link key"
                ),
            }
        } else {
            log::debug!("[BDB] step 12: TC link key exchange disabled, skipping");
        }

        self.bdb_node_is_on_a_network = true;
        self.bdb_commissioning_status = BdbCommissioningStatus::Success;
        Ok(confirm)
    }

    /// Broadcast a ZDO Device_annce (§2.4.3.1.11, BDB §8.2 step 11).
    async fn device_annce<M: Mlme>(
        device: &ZigbeeDevice<M>,
        capability_information: CapabilityInformation,
    ) -> Result<(), NetworkError> {
        let nib = nib::get_ref();
        let annce = DeviceAnnce {
            nwk_addr: ShortAddress(*nib.network_address()),
            ieee_addr: *nib.ieee_address(),
            capability: capability_information,
        };
        device.device_annce(annce).await
    }

    /// Negotiate the end-device timeout with the parent (§3.6.10.2).
    ///
    /// Sends an End Device Timeout Request; the concurrently running receive
    /// loop consumes the parent's response and updates the NIB. Non-fatal: a
    /// legacy parent never answers and keeps its default timeout.
    async fn negotiate_end_device_timeout<M: Mlme>(
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
    ) {
        log::debug!("[BDB] negotiate end-device timeout");
        if let Err(e) = device.nlme().send_end_device_timeout_request().await {
            log::warn!("[BDB] end-device timeout request failed ({e:?})");
            return;
        }

        // bounded wait for the receive loop to consume the response
        let nib = nib::get_ref();
        for _ in 0..3 {
            if device.nlme().negotiated_poll_interval_ms().is_some()
                || *nib.parent_information() != 0
            {
                return;
            }
            delay.delay_ms(500).await;
        }
        log::warn!("[BDB] no end-device timeout response, assuming legacy parent");
    }

    /// Trust Center link key exchange procedure (BDB §10.2.5).
    ///
    /// Replaces the default TC link key (key A) with a unique key (key B)
    /// through a three-phase exchange: REQUEST-KEY → TRANSPORT-KEY →
    /// VERIFY-KEY → CONFIRM-KEY.
    ///
    /// The receive loop consumes the Trust Center's replies and updates the
    /// AIB; this procedure only drives the timing: it sends the requests and
    /// awaits the stack's progress events for `bdbcTcLinkKeyExchangeTimeout`
    /// each.
    async fn tc_link_key_exchange<M: Mlme>(
        &mut self,
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
        &mut self,
        device: &ZigbeeDevice<M>,
        delay: &mut impl DelayNs,
    ) -> Result<(), NetworkError> {
        let tc_short = ShortAddress(0x0000);
        let aib = aib::get_ref();
        let tc_ieee = *aib.trust_center_address();

        log::debug!("[BDB] start TC link key exchange, TC={tc_ieee:?}");

        // §10.2.5 steps 6-9: the receive loop installs the transported key as
        // unverified and signals reception
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
                Some(()) => {
                    log::debug!("[BDB] received new TC link key");
                    break;
                }
                None if attempts >= BDBC_REC_SAME_NETWORK_RETRY_ATTEMPTS => {
                    log::warn!("[BDB] TC link key exchange failed: no TRANSPORT-KEY");
                    self.bdb_commissioning_status = BdbCommissioningStatus::TclkExFailure;
                    return Err(NetworkError::NoTransportKey);
                }
                None => continue,
            }
        }

        // §10.2.5 step 9: the unverified key is now in the AIB
        let new_key = aib
            .device_key_pair_set()
            .iter()
            .find(|k| k.device_address == tc_ieee)
            .map(|k| k.link_key)
            .ok_or(NetworkError::NoTransportKey)?;

        // §10.2.5 steps 10-13
        // §4.4.10.7.4
        let hash = HmacAes128Mmo::hmac(new_key.as_slice(), &[0x03]).map_err(|_| {
            NetworkError::SecurityError(zigbee::security::SecurityError::Unspecified)
        })?;
        // §4.4.10.7.3
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
                    self.bdb_commissioning_status = BdbCommissioningStatus::TclkExFailure;
                    return Err(NetworkError::NoTransportKey);
                }
            }
        }
    }

    fn is_end_device(&self) -> bool {
        self.config.device_type == LogicalType::EndDevice
    }

    fn is_router(&self) -> bool {
        self.config.device_type == LogicalType::Router
    }
}

/// resolve `fut`, or `None` if `timeout` fires first
async fn with_timeout<F: Future>(fut: F, timeout: impl Future<Output = ()>) -> Option<F::Output> {
    let mut fut = pin!(fut);
    let mut timeout = pin!(timeout);
    poll_fn(move |cx| {
        if let Poll::Ready(value) = fut.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        match timeout.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
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

    use zigbee::nwk::nib;
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
    // `try_init`/`reset` outside the `zigbee` crate itself), so every case
    // that needs it lives in one #[test] to keep initialization order
    // deterministic across the crate's test binary.
    #[test]
    fn start_initialization_procedure_early_returns() {
        nib::init();
        zigbee::aps::aib::init();

        // §7.1 step 2: not on a network (fresh NIB, network_address=0xffff)
        let device = make_device(LogicalType::EndDevice);
        let mut bdb = BaseDeviceBehavior::new(Config {
            device_type: LogicalType::EndDevice,
            ..Config::default()
        });
        let result = block_on(bdb.start_initialization_procedure(&device));
        assert!(matches!(result, Ok(None)));
        assert!(!bdb.bdb_node_is_on_a_network);

        // §7.1 step 3: on a network, but not an end device — no mac calls
        // are expected here since the router-side MockMlme has none set up
        nib::get_ref().update_network_address(|value| *value = 0x1234);
        let router_device = make_device(LogicalType::Router);
        let mut router_bdb = BaseDeviceBehavior::new(Config {
            device_type: LogicalType::Router,
            ..Config::default()
        });
        let result = block_on(router_bdb.start_initialization_procedure(&router_device));
        assert!(matches!(result, Ok(None)));
        assert!(router_bdb.bdb_node_is_on_a_network);
    }
}
