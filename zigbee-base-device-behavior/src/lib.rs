//! Implements the Zigbee Base Device Behavior (BDB) in `no-std` based on the
//! [ZigBee Base Device Behavior Specification Rev. 13].
//!
//! [ZigBee Base Device Behavior Specification Rev. 13]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
//!
//! This crate defines the standard commissioning procedures all devices must
//! support. It provides a high-level abstraction over the zigbee stack.
//!
//! Start with the [comissioning](commissioning/index.html)

use thiserror::Error;

pub mod commissioning { 
    pub mod network_steering;

    pub mod network_formation;

    pub mod finding_binding;

    /// For touchlink commissioning (if supported)
    pub mod touchlink;
}
pub mod leave;

pub mod rejoin;
pub mod reset;
pub mod types;

// Re-export key types for crate users
pub use commissioning::finding_binding::FindingBinding;
pub use commissioning::network_formation::NetworkFormation;
pub use commissioning::network_steering::NetworkSteering;
pub use commissioning::touchlink::TouchlinkCommissioning;
pub use leave::leave_network;
pub use rejoin::attempt_rejoin;
pub use reset::factory_reset;
pub use types::BdbCommissioningStatus;
pub use types::CommissioningMode;
pub use types::NetworkDescriptor;
use zigbee::zdo::ZigbeeDevice;

pub struct BaseDeviceBehavior {
    device: ZigbeeDevice,
    bdb_commisioning_capability: u8,
    logical_type: u8,
}

impl BaseDeviceBehavior {
    fn new(
        device: ZigbeeDevice,
        bdb_commisioning_capability: u8,
        logical_type: u8
        ) -> Self {
        Self {
            device,
            bdb_commisioning_capability,
            logical_type,
        }
    }

    /// Fig. 1 - Initialization procedure
    /// See Section 7.1
    fn start_initialization_procedure(&self) -> Result<(), ZigbeeError> {
        // TODO: restore persistent zigbee data

        if self.node_is_on_a_network() {
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
                }
            }
        }

        Ok(())
    }

    fn node_is_on_a_network(&self) -> bool {
        // TODO: check if a network info is stored in the persistent zigbee data
        true
    }

    fn is_end_device(&self) -> bool {
        self.logical_type == 0b010
    }

    fn is_router(&self) -> bool {
        self.logical_type == 0b001
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
pub enum ZigbeeError {
    #[error("invalid key")]
    NotSupported,
    #[error("invalid data")]
    Timeout,
    #[error("parse error")]
    Busy,
}

