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
#[allow(non_upper_case_globals)]
const bdbcMaxSameNetworkRetryAttempts: u8 = 10;
#[allow(non_upper_case_globals)]
const bdbcMinCommissioningTime: u8 = 0xb4;
#[allow(non_upper_case_globals)]
const bdbcRecSameNetworkRetryAttempts: u8 = 3;
#[allow(non_upper_case_globals)]
const bdbcTCLinkKeyExchangeTimeout: u8 = 5;

use types::BdbCommissioningStatus;
use types::CommissioningMode;
use zigbee::Config;
use zigbee::LogicalType;
use zigbee::nwk::nib;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nib::Nib;
use zigbee::nwk::nib::NibStorage;
use zigbee::nwk::nlme::NetworkError;
use zigbee::nwk::nlme::NlmeSap;
use zigbee::nwk::nlme::management::NlmeJoinConfirm;
use zigbee::nwk::nlme::management::NlmeJoinRequest;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee::nwk::nlme::management::NlmeNetworkFormationRequest;
use zigbee::nwk::nlme::management::NlmePermitJoiningRequest;
use zigbee::zdo::ZigbeeDevice;
use zigbee_types::IeeeAddress;

/// Base Device Behavior (BDB) commissioning manager.
///
/// Orchestrates the standard commissioning procedures defined in the
/// BDB specification: initialization, network steering, network
/// formation, finding & binding, and touchlink.
pub struct BaseDeviceBehavior<T: NlmeSap> {
    device: ZigbeeDevice,
    nlme: T,
    bdb_node_is_on_a_network: bool,
    bdb_commissioning_mode: CommissioningMode,
    bdb_commissioning_status: BdbCommissioningStatus,
}

impl<T: NlmeSap> BaseDeviceBehavior<T> {
    pub fn new(nlme: T, config: Config) -> Self {
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
        self.bdb_commissioning_status = BdbCommissioningStatus::InProgress;

        // BDB 8.2 step 1: NLME-NETWORK-DISCOVERY.request
        self.nlme
            .network_discovery(channels, scan_duration)
            .await?;

        // BDB 8.2 step 5: NLME-JOIN.request via MAC association
        let request = NlmeJoinRequest {
            extended_pan_id,
            rejoin_network: 0x00,
            scan_duration: 0x00,
            capability_information,
            security_enabled: false,
        };
        let confirm = self.nlme.join(request).await;
        if confirm.status != NlmeJoinStatus::Success {
            self.bdb_commissioning_status = BdbCommissioningStatus::NoNetwork;
            return Ok(confirm);
        }

        // BDB 8.2 step 12: wait for Trust Center to deliver the network key
        zigbee::aps::security::await_transport_key(&mut self.nlme).await?;

        self.bdb_node_is_on_a_network = true;
        self.bdb_commissioning_status = BdbCommissioningStatus::Success;
        Ok(confirm)
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
