//! Electrical Conductivity Measurement
//!
//! See Section 4.10
//!
//! Provides an interface to electrical conductivity measurement
//! functionality.

/// Cluster identifier (ZCL 4.10).
pub const CLUSTER_ID: u16 = 0x040a;

/// Reported when a value is unknown, and when a limit is not defined
/// (ZCL 4.10.2.1.1.1).
pub const UNKNOWN: u16 = 0xffff;

/// Attribute identifiers (ZCL 4.10.2.1).
pub mod attribute {
    /// `MeasuredValue` (`Uint16`, tenths of a milli-Siemens per metre).
    pub const MEASURED_VALUE: u16 = 0x0000;
    /// `MinMeasuredValue` (`Uint16`).
    pub const MIN_MEASURED_VALUE: u16 = 0x0001;
    /// `MaxMeasuredValue` (`Uint16`).
    pub const MAX_MEASURED_VALUE: u16 = 0x0002;
    /// `Tolerance` (`Uint16`).
    pub const TOLERANCE: u16 = 0x0003;
}

/// `MeasuredValue` of the Electrical Conductivity cluster
/// (ZCL 4.10.2.1.1.1).
///
/// Counted as 10 x the conductivity in mS/m, giving 0.1 resolution. Note the
/// scale differs from the other chapter 4 clusters, which use 100 x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectricalConductivity(u16);

impl ElectricalConductivity {
    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Conductivity in milli-Siemens per metre, `None` when unknown.
    pub fn milli_siemens_per_metre(self) -> Option<f32> {
        if self.0 == UNKNOWN {
            return None;
        }
        Some(f32::from(self.0) / 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 4.10.2.1.1.1: MeasuredValue = 10 x mS/m, not 100 x
    #[test]
    fn conductivity_is_scaled_by_ten() {
        assert_eq!(
            ElectricalConductivity::from_raw(0).milli_siemens_per_metre(),
            Some(0.0)
        );
        assert_eq!(
            ElectricalConductivity::from_raw(1234).milli_siemens_per_metre(),
            Some(123.4)
        );
    }

    #[test]
    fn unknown_reports_no_value() {
        assert_eq!(
            ElectricalConductivity::from_raw(UNKNOWN).milli_siemens_per_metre(),
            None
        );
    }
}
