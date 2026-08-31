//! Wind Speed Measurement Cluster
//!
//! See Section 4.12
//!
//! Provides an interface to wind speed measurement functionality, including
//! configuration and provision of notifications of wind speed measurements.
use core::convert::TryInto;

use heapless::Vec;

/// Cluster identifier (ZCL 4.12).
pub const CLUSTER_ID: u16 = 0x040b;

/// Reported when a value is unknown, and when a limit is not defined
/// (ZCL 4.12.2.1.1).
pub const UNKNOWN: u16 = 0xffff;

/// Largest value `MaxMeasuredValue` may take (ZCL 4.12.2.1.3).
pub const MAX_MEASURED_VALUE_LIMIT: u16 = 0xfffe;

/// Attribute identifiers (ZCL 4.12.2.1).
pub mod attribute {
    /// `MeasuredValue` (`Uint16`, hundredths of a metre per second).
    pub const MEASURED_VALUE: u16 = 0x0000;
    /// `MinMeasuredValue` (`Uint16`).
    pub const MIN_MEASURED_VALUE: u16 = 0x0001;
    /// `MaxMeasuredValue` (`Uint16`).
    pub const MAX_MEASURED_VALUE: u16 = 0x0002;
    /// `Tolerance` (`Uint16`).
    pub const TOLERANCE: u16 = 0x0003;
}

/// Wind Speed Measurement Information Attribute Set
///
/// See Section 4.12.2.1
#[derive(Debug)]
pub struct WindSpeedMeasurement {
    measured_value: u16,     // MeasuredValue in 0.01 m/s units
    min_measured_value: u16, // MinMeasuredValue
    max_measured_value: u16, // MaxMeasuredValue
    tolerance: u16,          // Tolerance (optional, set to 0 if not used)
}

impl WindSpeedMeasurement {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn new(
        speed_ms: f32,
        min_speed: f32,
        max_speed: f32,
        tolerance: u16,
    ) -> Result<Self, &'static str> {
        if speed_ms < 0.0 || min_speed < 0.0 || max_speed < 0.0 {
            return Err("Wind speed cannot be negative");
        }
        // 0xffff is reserved for "unknown", so the largest representable
        // maximum is 0xfffe (4.12.2.1.3)
        if max_speed * 100.0 > f32::from(MAX_MEASURED_VALUE_LIMIT) {
            return Err("Wind speed cannot exceed 655.34 m/s");
        }
        if min_speed >= max_speed {
            return Err("Min speed must be below max speed");
        }
        if speed_ms < min_speed || speed_ms > max_speed {
            return Err("Measured wind speed is out of the defined range");
        }

        let measured_value = (speed_ms * 100.0) as u16;
        let min_measured_value = (min_speed * 100.0) as u16;
        let max_measured_value = (max_speed * 100.0) as u16;

        Ok(Self {
            measured_value,
            min_measured_value,
            max_measured_value,
            tolerance,
        })
    }

    /// Reports every value as unknown, for a sensor that has not yet produced
    /// a reading (4.12.2.1.1).
    pub const fn unknown() -> Self {
        Self {
            measured_value: UNKNOWN,
            min_measured_value: UNKNOWN,
            max_measured_value: UNKNOWN,
            tolerance: 0,
        }
    }

    /// Measured wind speed in metres per second, `None` while unknown.
    pub fn speed_ms(&self) -> Option<f32> {
        if self.measured_value == UNKNOWN {
            return None;
        }
        Some(f32::from(self.measured_value) / 100.0)
    }

    pub fn to_bytes(&self) -> Vec<u8, 8> {
        let mut bytes = Vec::new();
        bytes
            .extend_from_slice(&self.measured_value.to_le_bytes())
            .unwrap();
        bytes
            .extend_from_slice(&self.min_measured_value.to_le_bytes())
            .unwrap();
        bytes
            .extend_from_slice(&self.max_measured_value.to_le_bytes())
            .unwrap();
        bytes
            .extend_from_slice(&self.tolerance.to_le_bytes())
            .unwrap();
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 8 {
            return Err("Invalid byte slice length");
        }

        let measured_value = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
        let min_measured_value = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
        let max_measured_value = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        let tolerance = u16::from_le_bytes(bytes[6..8].try_into().unwrap());

        Ok(Self {
            measured_value,
            min_measured_value,
            max_measured_value,
            tolerance,
        })
    }

    pub fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        let bytes: Vec<u8, 8> = src.into_iter().collect();
        Self::from_bytes(&bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wind_speed_measurement() {
        let wind_speed_measurement =
            WindSpeedMeasurement::new(12.5, 0.0, 100.0, 2).expect("Initialization failed");
        let serialized = wind_speed_measurement.to_bytes();
        let deserialized =
            WindSpeedMeasurement::from_bytes(&serialized).expect("Deserialization failed");
        assert_eq!(
            wind_speed_measurement.measured_value,
            deserialized.measured_value
        );
        assert_eq!(
            wind_speed_measurement.min_measured_value,
            deserialized.min_measured_value
        );
        assert_eq!(
            wind_speed_measurement.max_measured_value,
            deserialized.max_measured_value
        );
        assert_eq!(wind_speed_measurement.tolerance, deserialized.tolerance);
    }

    // 4.12.2.1.1.1: MeasuredValue is 100 x the speed in m/s
    #[test]
    fn measured_value_is_scaled_by_one_hundred() {
        let measurement = WindSpeedMeasurement::new(12.5, 0.0, 100.0, 0).unwrap();
        assert_eq!(measurement.measured_value, 1250);
        assert_eq!(measurement.speed_ms(), Some(12.5));
    }

    // 4.12.2.1.1: 0xffff means unknown, and reads back as no value
    #[test]
    fn unknown_reports_no_speed() {
        assert_eq!(WindSpeedMeasurement::unknown().speed_ms(), None);
    }

    #[test]
    fn rejects_values_outside_the_representable_range() {
        assert!(WindSpeedMeasurement::new(-1.0, 0.0, 100.0, 0).is_err());
        assert!(WindSpeedMeasurement::new(10.0, 0.0, 700.0, 0).is_err());
        assert!(WindSpeedMeasurement::new(10.0, 50.0, 20.0, 0).is_err());
        assert!(WindSpeedMeasurement::new(150.0, 0.0, 100.0, 0).is_err());
    }
}
