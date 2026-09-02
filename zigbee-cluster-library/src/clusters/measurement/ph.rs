//! pH Measurement
//!
//! See Section 4.11
//!
//! Provides an interface to pH measurement functionality.

/// Cluster identifier (ZCL 4.11).
pub const CLUSTER_ID: u16 = 0x0409;

/// Reported when a value is unknown, and when a limit is not defined
/// (ZCL 4.11.2.1.1.1).
pub const UNKNOWN: u16 = 0xffff;

/// `MeasuredValue` at pH 14.00, the top of the scale (ZCL 4.11.2.1.1.1).
pub const FULL_SCALE: u16 = 0x0578;

/// Attribute identifiers (ZCL 4.11.2.1).
pub mod attribute {
    /// `MeasuredValue` (`Uint16`, hundredths of a pH).
    pub const MEASURED_VALUE: u16 = 0x0000;
    /// `MinMeasuredValue` (`Uint16`).
    pub const MIN_MEASURED_VALUE: u16 = 0x0001;
    /// `MaxMeasuredValue` (`Uint16`).
    pub const MAX_MEASURED_VALUE: u16 = 0x0002;
    /// `Tolerance` (`Uint16`).
    pub const TOLERANCE: u16 = 0x0003;
}

/// `MeasuredValue` of the pH Measurement cluster (ZCL 4.11.2.1.1.1).
///
/// Counted as 100 x the pH, which is unitless, giving 0.01 resolution over
/// the range 0.00 to 14.00.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ph(u16);

impl Ph {
    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// pH, `None` when the reading is unknown.
    pub fn ph(self) -> Option<f32> {
        if self.0 == UNKNOWN {
            return None;
        }
        Some(f32::from(self.0) / 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 4.11.2.1.1.1: MeasuredValue = 100 x pH, 0x0000 to 0x0578
    #[test]
    fn ph_is_scaled_by_one_hundred() {
        assert_eq!(Ph::from_raw(0).ph(), Some(0.0));
        assert_eq!(Ph::from_raw(725).ph(), Some(7.25));
        assert_eq!(Ph::from_raw(FULL_SCALE).ph(), Some(14.0));
    }

    #[test]
    fn unknown_reports_no_value() {
        assert_eq!(Ph::from_raw(UNKNOWN).ph(), None);
    }
}
