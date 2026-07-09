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
}

impl BaseDeviceBehavior {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            bdb_node_is_on_a_network: false,
            bdb_commissioning_mode: CommissioningMode::NetworkSteering,
            bdb_commissioning_status: BdbCommissioningStatus::Success,
        }
    }

    /// Returns a reference to the global NIB singleton.
    pub fn nib(&self) -> &'static Nib {
        nib::get_ref()
    }

    /// Initialization procedure (BDB §7.1).
    ///
    /// Restores persistent state and, if the node is already on a network,
    /// attempts to rejoin it. Returns without error if the node is not on
    /// a network — the caller should then invoke [`network_steering`].
    pub async fn start_initialization_procedure<M: Mlme>(
        &mut self,
        _device: &ZigbeeDevice<M>,
    ) -> Result<(), NetworkError> {
        // §7.1 step 1: restore persistent state (NIB/AIB backed by storage)
        // §7.1 steps 2-8: TODO implement rejoin path
        Ok(())
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

        // §8.2 step 11
        log::debug!("[BDB] step 11: broadcast Device_annce");
        Self::device_annce(device, capability_information).await?;

        // §8.2 step 12, §10.2.5
        log::debug!("[BDB] step 12: TC link key exchange");
        // non-fatal: a TC allowing legacy devices may never answer the
        // REQUEST-KEY, keep the default global TC link key then
        match self.tc_link_key_exchange(device, delay).await {
            Ok(()) => log::debug!("[BDB] step 12: TC link key exchange complete"),
            Err(e) => log::warn!(
                "[BDB] step 12: TC link key exchange failed ({e:?}), continuing with default TC link key"
            ),
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
                Some(()) => {
                    log::debug!("[BDB] TC link key verified successfully");
                    return Ok(());
                }
                None if attempts >= BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS => {
                    log::warn!("[BDB] TC link key exchange failed: no CONFIRM-KEY");
                    self.bdb_commissioning_status = BdbCommissioningStatus::TclkExFailure;
                    return Err(NetworkError::NoTransportKey);
                }
                None => continue,
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
