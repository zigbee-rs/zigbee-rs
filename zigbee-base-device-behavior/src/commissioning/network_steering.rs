//! Handles scanning / joining a network
//!
//! See Section 5.2.2
//!
//! Handles scanning for and joining a Zigbee network as per BDB commissioning.

use crate::types::BdbCommissioningStatus;

/// Implements the BDB Network Steering commissioning mode.
///
/// See Section 5.2.2.
pub struct NetworkSteering;

impl NetworkSteering {
    /// Starts the network steering process.
    ///
    /// See Section 5.2.2.
    pub fn start() -> Result<BdbCommissioningStatus, ()> {
        Ok(BdbCommissioningStatus::Success)
    }
}
