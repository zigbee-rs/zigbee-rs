//! Implements the Zigbee Base Device Behavior (BDB) in `no-std` based on the
//! [ZigBee Base Device Behavior Specification Rev. 13].
//!
//! [ZigBee Base Device Behavior Specification Rev. 13]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
//!
//! This crate defines the standard commissioning procedures all devices must
//! support. It provides a high-level abstraction over the zigbee stack.
//!
//! Start with the [comissioning](commissioning/index.html)
//!
//! ```no_test
//! use zigbee_base_device_behavior::BaseDeviceBehavior;
//! let behavior = BaseDeviceBehavior::new();
//! let _ = behavior.start_initialization_procedure();
//! ```
#![no_std]
#![allow(unused)]

use byte::BytesExt;
use spin::Mutex;

use embedded_storage::Storage;
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

use types::CommissioningMode;
use types::BdbCommissioningStatus;
use zigbee::nwk::nlme::management::NlmeJoinRequest;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee::nwk::nlme::management::NlmeNetworkFormationRequest;
use zigbee::nwk::nlme::management::NlmePermitJoiningRequest;
use zigbee::nwk::nlme::NlmeSap;
use zigbee::{zdo::ZigbeeDevice, Config, LogicalType};

pub struct BaseDeviceBehavior<'a, C, T: NlmeSap> {
    storage: Mutex<C>,
    device: ZigbeeDevice,
    nlme: &'a T,
    bdb_node_is_on_a_network: bool,
    bdb_commissioning_mode: CommissioningMode,
    bdb_commisioning_capability: u8,
    bdb_commissioning_status: BdbCommissioningStatus,
}

impl<'a, C, T> BaseDeviceBehavior<'a, C, T> where 
    C: Storage, 
    T: NlmeSap 
{
    pub fn new(
        storage: C,
        nlme: &'a T,
        config: Config,
        bdb_commisioning_capability: u8,
        ) -> Self {
        let device = ZigbeeDevice::new(config);
 
        Self {
            storage: Mutex::new(storage),
            device,
            nlme,
            bdb_node_is_on_a_network: false,
            bdb_commissioning_mode: CommissioningMode::NetworkSteering,
            bdb_commisioning_capability,
            bdb_commissioning_status: BdbCommissioningStatus::Success,
        }
    }

    /// Initialization procedure
    ///
    /// A node performs initialization whenever it is supplied with power either the first time or 
    /// subsequent times after some form of power outage or power-cycle.
    ///
    /// See Section 7.1 - Figure 1 
    pub fn start_initialization_procedure(&mut self) -> Result<(), ZigbeeError> {
        // restore persistent zigbee data
        let mut buf = [0u8; 1];
        let _ = self.storage.lock().read(0, &mut buf);
        self.bdb_node_is_on_a_network = buf.read_with(&mut 0, ()).unwrap();

        if self.bdb_node_is_on_a_network {
            log::error!("bdb_node_is_on_a_network");

            return match self.device.logical_type() {
                LogicalType::EndDevice => {
                    let result = self.attempt_to_rejoin();
                    if result.is_ok() {
                        self.broadcast_annce()
                    } else {
                        // TODO: retry the procedure at some application specific time or quit
                        
                        Ok(())
                    }
                },
                LogicalType::Router => Ok(()),
                _ => Ok(())
            }
        } else if self.is_router() && self.is_touchlink_supported() {
            log::error!("is router and touchlink supported");

            // TODO: select a channel from bdbcTLPrimaryChannelSetNoYesStep

        } else {
            log::error!("not on a network");

            let request = NlmeNetworkFormationRequest {
            };
            self.nlme.network_formation(request);
        }
        Ok(())
    }

    /// Network steering procedure
    ///
    /// See Section 8.2
    pub fn start_network_steering(&mut self) -> Result<BdbCommissioningStatus, SteeringError> {
        if self.bdb_node_is_on_a_network {
            // Section 8.1 | for a node on a network
            self.bdb_commissioning_status = BdbCommissioningStatus::InProgress;

            // TODO: Mgmt_permit_join_request | zigbee 2.4.3.3.7
            // with PermitDuration set to at least bdbcMinCommissioningTime
            // with TC_Significance field set to 0x01
            if self.device.logical_type() == LogicalType::Coordinator ||
                self.device.logical_type() == LogicalType::Router {
                    // TODO: enable permit join >= bdbcMinCommissioningTime seconds
                    //
                    let request = NlmePermitJoiningRequest {
                        permit_duration: bdbcMinCommissioningTime
                    };

                    let confirm = self.nlme.permit_joining(request);
                }

            self.bdb_commissioning_status = BdbCommissioningStatus::Success;
        } else {
            // Section 8.2 | for a node NOT on a network
            self.bdb_commissioning_status = BdbCommissioningStatus::InProgress;

            // TODO: perform network discovery over the channels vScanChannels
            // if success determine a list of suitable open networks
            // join network using MAC association
            // if success wait for network key
        }

        Ok(BdbCommissioningStatus::Success)
    }

    fn is_end_device(&self) -> bool {
        self.device.logical_type() == LogicalType::EndDevice
    }

    fn is_router(&self) -> bool {
        self.device.logical_type() == LogicalType::Router
    }

    fn is_touchlink_supported(&self) -> bool {
        self.bdb_commisioning_capability == 1
    }

    fn attempt_to_rejoin(&self) -> Result<(), ZigbeeError> {
        let confirm = self.nlme.rejoin();

        if confirm.status == NlmeJoinStatus::Success {
            Ok(())
        } else {
            Err(ZigbeeError::NotSupported)
        }
    }

    /// Trigger Device_annce on ZDO command 
    fn broadcast_annce(&self) -> Result<(), ZigbeeError> {
        // TODO: Device_annce should be sent by NWK automatically after a device has
        // joined/rejoined 
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SteeringError {
    #[error("Trust center link key exchange failed")]
    NwkJoinFailure,

    #[error("No open network discovered to join")]
    NoNetwork,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ZigbeeError {
    #[error("invalid key")]
    NotSupported,
    #[error("invalid data")]
    Timeout,
    #[error("parse error")]
    Busy,
}

