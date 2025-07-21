//! BDB Commissioning State Machine
//!
//! See Figure 5-1 and Section 5.2
//!
//! Implements the central state machine for the Zigbee Base Device Behavior
//! (BDB) commissioning flow.

/// Central state machine for BDB commissioning flow.
///
/// See Figure 5-1.
pub struct BdbCommissioningStateMachine;

impl BdbCommissioningStateMachine {
    /// Creates a new BDB commissioning state machine.
    pub fn new() -> Self {
        Self
    }
    /// Starts the commissioning process.
    ///
    /// See Section 5.2.
    pub fn start_commissioning(&mut self) {
        // Stub: do nothing
    }
}
