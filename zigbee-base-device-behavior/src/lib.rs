//! Implements the Zigbee Base Device Behavior (BDB) in `no-std` based on the
//! [ZigBee Base Device Behavior Specification Rev. 13].
//!
//! [ZigBee Base Device Behavior Specification Rev. 13]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
//!
//! This crate defines the standard commissioning procedures all devices must
//! support. It provides a high-level abstraction over the zigbee stack.
#![no_std]
#![allow(unused)]

use thiserror::Error;

pub mod types;

// BDB 5.1 | Table 1
const BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS: u8 = 10;
const BDBC_MIN_COMMISSIONING_TIME: u8 = 0xb4;
const BDBC_REC_SAME_NETWORK_RETRY_ATTEMPTS: u8 = 3;
const BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT: u8 = 5;

use types::BdbCommissioningStatus;
use types::CommissioningMode;
use zigbee::Config;
use zigbee::LogicalType;
use zigbee::aps::aib;
use zigbee::aps::aib::DeviceKeyPairDescriptor;
use zigbee::aps::aib::KeyAttribute;
use zigbee::aps::aib::LinkKeyType;
use zigbee::aps::frame::command::Command;
use zigbee::aps::frame::command::ConfirmKey;
use zigbee::aps::frame::command::RequestKey;
use zigbee::aps::frame::command::TransportKey;
use zigbee::aps::frame::command::VerifyKey;
use zigbee::nwk::nib;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nib::Nib;
use zigbee::nwk::nib::NibStorage;
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
/// Orchestrates the standard commissioning procedures defined in the
/// BDB specification: initialization, network steering, network
/// formation, finding & binding, and touchlink.
pub struct BaseDeviceBehavior<M: Mlme> {
    device: ZigbeeDevice,
    nlme: Nlme<M>,
    bdb_node_is_on_a_network: bool,
    bdb_commissioning_mode: CommissioningMode,
    bdb_commissioning_status: BdbCommissioningStatus,
}

impl<M: Mlme> BaseDeviceBehavior<M> {
    pub fn new(nlme: Nlme<M>, config: Config) -> Self {
        let device = ZigbeeDevice::new(config);

        Self {
            device,
            nlme,
            bdb_node_is_on_a_network: false,
            bdb_commissioning_mode: CommissioningMode::NetworkSteering,
            bdb_commissioning_status: BdbCommissioningStatus::Success,
        }
    }

    /// Returns a reference to the global NIB singleton.
    pub fn nib(&self) -> &'static Nib<NibStorage> {
        nib::get_ref()
    }

    /// Initialization procedure (BDB §7.1).
    ///
    /// Restores persistent state and, if the node is already on a network,
    /// attempts to rejoin it. Returns without error if the node is not on
    /// a network — the caller should then invoke [`network_steering`].
    pub async fn start_initialization_procedure(&mut self) -> Result<(), NetworkError> {
        // §7.1 step 1: restore persistent state (NIB/AIB backed by storage)
        // §7.1 steps 2-8: TODO implement rejoin path
        Ok(())
    }

    /// Network steering procedure for a node NOT on a network
    /// (BDB §8.2).
    ///
    /// Performs NLME-NETWORK-DISCOVERY on the given channels, then
    /// NLME-JOIN for the specified extended PAN ID, and finally the
    /// APS transport key exchange to obtain the network key from the
    /// Trust Center.
    pub async fn network_steering(
        &mut self,
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
        self.nlme.network_discovery(channels, scan_duration).await?;

        // §8.2 step 5
        let request = NlmeJoinRequest {
            extended_pan_id,
            rejoin_network: RejoinNetwork::Association,
            capability_information,
            security_enabled: false,
        };
        let confirm = self.nlme.join(request).await;
        if confirm.status != NlmeJoinStatus::Success {
            self.bdb_commissioning_status = BdbCommissioningStatus::NoNetwork;
            return Ok(confirm);
        }

        // §8.2 step 9
        self.device.poll_transport_key(&mut self.nlme).await?;

        // §8.2 step 11
        self.device_annce(capability_information).await?;

        // §8.2 step 12, §10.2.5
        self.tc_link_key_exchange().await?;

        self.bdb_node_is_on_a_network = true;
        self.bdb_commissioning_status = BdbCommissioningStatus::Success;
        Ok(confirm)
    }

    /// Broadcast a ZDO Device_annce (§2.4.3.1.11, BDB §8.2 step 11).
    async fn device_annce(
        &mut self,
        capability_information: CapabilityInformation,
    ) -> Result<(), NetworkError> {
        let nib = nib::get_ref();
        let annce = DeviceAnnce {
            nwk_addr: ShortAddress(nib.network_address()),
            ieee_addr: nib.ieee_address(),
            capability: capability_information,
        };
        self.device.device_annce(&mut self.nlme, annce).await
    }

    /// Trust Center link key exchange procedure (BDB §10.2.5).
    ///
    /// Replaces the default TC link key (key A) with a unique key (key B)
    /// through a three-phase exchange: REQUEST-KEY → TRANSPORT-KEY →
    /// VERIFY-KEY → CONFIRM-KEY.
    async fn tc_link_key_exchange(&mut self) -> Result<(), NetworkError> {
        let tc_short = ShortAddress(0x0000);
        let tc_ieee = aib::get_ref().trust_center_address();

        log::debug!("[BDB] start TC link key exchange, TC={tc_ieee:?}");

        // §10.2.5 steps 6-9
        let mut attempts = 0u8;
        let new_key = loop {
            log::debug!("[BDB] send_aps_command");
            self.device
                .send_aps_command(
                    &mut self.nlme,
                    tc_short,
                    tc_ieee,
                    Command::RequestKey(RequestKey::TrustCenterLinkKey),
                    true,
                )
                .await?;
            attempts += 1;
            log::debug!("[BDB] send_aps_command ok");

            match self
                .device
                .poll_aps_command(&mut self.nlme, BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT)
                .await
            {
                Ok(Command::TransportKey(TransportKey::TrustCenterLinkKey(key_desc))) => {
                    log::debug!("[BDB] received new TC link key");
                    break key_desc.key;
                }
                _ if attempts >= BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS => {
                    log::warn!("[BDB] TC link key exchange failed: no TRANSPORT-KEY");
                    self.bdb_commissioning_status = BdbCommissioningStatus::TclkExFailure;
                    return Err(NetworkError::NoTransportKey);
                }
                _ => continue,
            }
        };

        // §10.2.5 step 9
        let aib = aib::get_ref();
        let mut key_set = aib.device_key_pair_set();
        if let Some(entry) = key_set.iter_mut().find(|k| k.device_address == tc_ieee) {
            entry.link_key = new_key;
            entry.key_attributes = KeyAttribute::UnverifiedKey;
            entry.outgoing_frame_counter = 0;
            entry.incoming_frame_counter = 0;
        } else {
            let _ = key_set.push(DeviceKeyPairDescriptor {
                device_address: tc_ieee,
                key_attributes: KeyAttribute::UnverifiedKey,
                link_key: new_key,
                outgoing_frame_counter: 0,
                incoming_frame_counter: 0,
                link_key_type: LinkKeyType::UniqueLinkKey,
            });
        }
        aib.set_device_key_pair_set(key_set);

        // §10.2.5 steps 10-13
        // §4.4.10.7.4
        let hash = HmacAes128Mmo::hmac(new_key.as_slice(), &[0x03]).map_err(|_| {
            NetworkError::SecurityError(zigbee::security::SecurityError::Unspecified)
        })?;
        // §4.4.10.7.3
        let device_addr = nib::get_ref().ieee_address();

        let mut attempts = 0u8;
        loop {
            self.device
                .send_aps_command(
                    &mut self.nlme,
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

            match self
                .device
                .poll_aps_command(&mut self.nlme, BDBC_TC_LINK_KEY_EXCHANGE_TIMEOUT)
                .await
            {
                Ok(Command::ConfirmKey(confirm)) if confirm.status == 0x00 => {
                    log::debug!("[BDB] TC link key verified successfully");
                    // mark key as verified
                    let mut key_set = aib.device_key_pair_set();
                    if let Some(entry) = key_set.iter_mut().find(|k| k.device_address == tc_ieee) {
                        entry.key_attributes = KeyAttribute::VerifiedKey;
                    }
                    aib.set_device_key_pair_set(key_set);
                    return Ok(());
                }
                _ if attempts >= BDBC_MAX_SAME_NETWORK_RETRY_ATTEMPTS => {
                    log::warn!("[BDB] TC link key exchange failed: no CONFIRM-KEY");
                    self.bdb_commissioning_status = BdbCommissioningStatus::TclkExFailure;
                    return Err(NetworkError::NoTransportKey);
                }
                _ => continue,
            }
        }
    }

    fn is_end_device(&self) -> bool {
        self.device.logical_type() == LogicalType::EndDevice
    }

    fn is_router(&self) -> bool {
        self.device.logical_type() == LogicalType::Router
    }
}

#[derive(Debug, Error)]
pub enum BdbError {
    #[error("network error")]
    NetworkError(#[from] NetworkError),

    #[error("no open network discovered to join")]
    NoNetwork,
}
