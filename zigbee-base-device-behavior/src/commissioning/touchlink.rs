//! Touchlink Commissioning Mode
//!
//! See Section 5.2.5
//!
//! Handles proximity-based commissioning as per BDB commissioning.

use crate::types::BdbCommissioningStatus;

/// Implements the BDB Touchlink commissioning mode.
///
/// See Section 5.2.5.
pub struct TouchlinkCommissioning;

impl TouchlinkCommissioning {
    /// Starts the touchlink commissioning process.
    ///
    /// See Section 5.2.5.
    pub fn start() -> Result<BdbCommissioningStatus, ()> {
        Ok(BdbCommissioningStatus::Success)
    }
}
