//! Implements the Zigbee Base Device Behavior (BDB) in `no-std` based on the
//! [ZigBee Base Device Behavior Specification Rev. 13].
//!
//! [ZigBee Base Device Behavior Specification Rev. 13]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
//!
//! This crate defines the standard commissioning procedures all devices must
//! support. It provides a high-level abstraction over the zigbee stack.

pub mod commissioning {
    pub mod finding_binding;

    pub mod network_formation;
    pub mod network_steering;
    pub mod touchlink;
}
pub mod bdb_state_machine;
pub mod leave;

pub mod rejoin;
pub mod reset;
pub mod types;

mod zigbee_stack;

// Re-export key types for crate users
pub use bdb_state_machine::BdbCommissioningStateMachine;
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
pub use zigbee_stack::NetworkFormationConfig;
pub use zigbee_stack::ZigbeeStack;
pub use zigbee_stack::ZigbeeStackError;
