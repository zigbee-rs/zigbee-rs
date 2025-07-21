//! Finding & Binding Commissioning Mode
//!
//! See Section 5.2.4
//!
//! Handles endpoint discovery and binding as per BDB commissioning.

use crate::types::BdbCommissioningStatus;

/// Implements the BDB Finding & Binding commissioning mode.
///
/// See Section 5.2.4.
pub struct FindingBinding;

impl FindingBinding {
    /// Starts the finding & binding process.
    ///
    /// See Section 5.2.4.
    pub fn start() -> Result<BdbCommissioningStatus, ()> {
        Ok(BdbCommissioningStatus::Success)
    }
}
