//! Water Content Measurement
//!
//! See Section 4.7
//!
//! Provides an interface to water content measurement functionality. One
//! attribute set serves three cluster identifiers, which differ only in what
//! the percentage is measured in: the air, the leaves of plants, or the soil.

/// Relative Humidity — percentage of water in the air (ZCL 4.7).
pub const RELATIVE_HUMIDITY_CLUSTER_ID: u16 = 0x0405;
/// Leaf Wetness — percentage of water on the leaves of plants (ZCL 4.7).
pub const LEAF_WETNESS_CLUSTER_ID: u16 = 0x0407;
/// Soil Moisture — percentage of water in the soil (ZCL 4.7).
pub const SOIL_MOISTURE_CLUSTER_ID: u16 = 0x0408;

/// Reported when a value is unknown, and when a limit is not defined
/// (ZCL 4.7.2.1.1).
pub const UNKNOWN: u16 = 0xffff;

/// `MeasuredValue` at 100% water content (ZCL 4.7.2.1.1).
pub const FULL_SCALE: u16 = 0x2710;

/// Attribute identifiers (ZCL 4.7.2.1).
pub mod attribute {
    /// `MeasuredValue` (`Uint16`, hundredths of a percent).
    pub const MEASURED_VALUE: u16 = 0x0000;
    /// `MinMeasuredValue` (`Uint16`).
    pub const MIN_MEASURED_VALUE: u16 = 0x0001;
    /// `MaxMeasuredValue` (`Uint16`).
    pub const MAX_MEASURED_VALUE: u16 = 0x0002;
    /// `Tolerance` (`Uint16`).
    pub const TOLERANCE: u16 = 0x0003;
}

/// `MeasuredValue` of any of the three water content clusters
/// (ZCL 4.7.2.1.1).
///
/// Counted as 100 x the percentage, so 100% is `0x2710`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterContent(u16);

impl WaterContent {
    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Water content in percent, `None` when the reading is unknown.
    pub fn percent(self) -> Option<f32> {
        if self.0 == UNKNOWN {
            return None;
        }
        Some(f32::from(self.0) / 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 4.7.2.1.1: MeasuredValue = 100 x water content, 0 to 0x2710
    #[test]
    fn water_content_is_scaled_by_one_hundred() {
        assert_eq!(WaterContent::from_raw(0).percent(), Some(0.0));
        assert_eq!(WaterContent::from_raw(4550).percent(), Some(45.5));
        assert_eq!(WaterContent::from_raw(FULL_SCALE).percent(), Some(100.0));
    }

    #[test]
    fn unknown_reports_no_value() {
        assert_eq!(WaterContent::from_raw(UNKNOWN).percent(), None);
    }
}
