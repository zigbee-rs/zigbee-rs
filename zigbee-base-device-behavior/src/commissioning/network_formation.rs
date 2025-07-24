//! Forms a ZigBee network (coordinator only)
//!
//! See Section 5.2.3
//!
//! Handles forming a new Zigbee network as per BDB commissioning.

use crate::types::BdbCommissioningStatus;

/// Implements the BDB Network Formation commissioning mode.
///
/// See Section 5.2.3.
pub struct NetworkFormation;

impl NetworkFormation {
    /// Starts the network formation process.
    ///
    /// See Section 5.2.3.
    pub fn start() -> Result<BdbCommissioningStatus, ()> {
        Ok(BdbCommissioningStatus::Success)
    }
}
