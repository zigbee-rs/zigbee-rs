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
//! ```rust
//! let behavior = BaseDeviceBehavior::new();
//! let _ = behavior.start_initialization_procedure();
//! ```

use byte::BytesExt;
use spin::Mutex;

use embedded_storage::Storage;
use embedded_storage::ReadStorage;
use thiserror::Error;

pub mod types;

use types::{BdbCommissioningStatus, CommissioningMode};
use zigbee::{zdo::ZigbeeDevice, Config, LogicalType};

pub struct BaseDeviceBehavior<C> {
    storage: Mutex<C>,
    device: ZigbeeDevice,
    bdb_commissioning_mode: CommissioningMode,
    bdb_commisioning_capability: u8,
    bdb_commissioning_status: BdbCommissioningStatus,
}

impl<C: Storage> BaseDeviceBehavior<C> {
    fn new(
        storage: C,
        config: Config,
        bdb_commisioning_capability: u8,
        ) -> Self {
        let device = ZigbeeDevice::new(config);
 
        Self {
            storage: Mutex::new(storage),
            device,
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
    fn start_initialization_procedure(&self) -> Result<(), ZigbeeError> {
        // TODO: restore persistent zigbee data
        let mut buf = [0u8; 1];
        let _ = self.storage.lock().read(0, &mut buf);
        buf.read_with(&mut 0, ()).unwrap()

        if self.node_is_on_a_network() {
            // TODO: done
            if self.is_end_device() {
                let result = self.attempt_to_rejoin();
                if result.is_ok() {
                    self.broadcast_annce();
                }
            };
        } else {
            if self.is_router() {
                if self.is_touchlink_supported() {
                    // TODO: select a channel from bdbcTLPrimaryChannelSetNoYesStep
                    unimplemented!()
                }
            }
        }

        Ok(())
    }

    /// Starts the network steering process.
    ///
    /// See Section 8.2
    pub fn start_network_steering(&mut self) -> Result<BdbCommissioningStatus, SteeringError> {
        self.bdb_commissioning_status = BdbCommissioningStatus::InProgress;

        Ok(BdbCommissioningStatus::Success)
    }


    fn node_is_on_a_network(&self) -> bool {
        // TODO: check if a network info is stored in the persistent zigbee data
        true
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
        unimplemented!()
    }

    fn broadcast_annce(&self) -> Result<(), ZigbeeError> {
        unimplemented!()
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

