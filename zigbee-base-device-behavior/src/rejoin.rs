//! Rejoin
//!
//! See Section 5.2.6
//!
//! Handles rejoining a Zigbee network as per BDB commissioning.

use crate::types::BdbCommissioningStatus;

/// Attempts to rejoin the network.
///
/// See Section 5.2.6.
pub fn attempt_rejoin() -> Result<BdbCommissioningStatus, ()> {
    Ok(BdbCommissioningStatus::Success)
}
