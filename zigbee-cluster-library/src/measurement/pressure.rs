//! Pressure Measurement Cluster
//!
//! See Section 4.5
//!
//! Provides an interface to pressure measurement functionality, including configuration and
//! provision of notifications of pressure measurements.
use core::convert::TryInto;
use heapless::Vec;

/// Pressure Measurement Information Attribute Set
///
/// See Section 4.5.2.2.1
#[derive(Debug)]
pub struct PressureMeasurement {
    measured_value: i16,     // MeasuredValue in 0.1 kPa units
    min_measured_value: i16, // MinMeasuredValue
    max_measured_value: i16, // MaxMeasuredValue
    tolerance: u16,          // Tolerance (optional, set to 0 if not used)
}

impl PressureMeasurement {
    pub fn new(
        pressure_kpa: f32,
        min_pressure: f32,
        max_pressure: f32,
        tolerance: u16,
    ) -> Result<Self, &'static str> {
        if pressure_kpa < -3276.7 || min_pressure < -3276.7 || max_pressure < -3276.7 {
            return Err("Pressure cannot be below -3276.7 kPa");
        }
        if min_pressure > max_pressure {
            return Err("Min pressure cannot be greater than max pressure");
        }
        if pressure_kpa < min_pressure || pressure_kpa > max_pressure {
            return Err("Measured pressure is out of the defined range");
        }

        let measured_value = (pressure_kpa * 10.0) as i16;
        let min_measured_value = (min_pressure * 10.0) as i16;
        let max_measured_value = (max_pressure * 10.0) as i16;

        Ok(Self {
            measured_value,
            min_measured_value,
            max_measured_value,
            tolerance,
        })
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

        let measured_value = i16::from_le_bytes(bytes[0..2].try_into().unwrap());
        let min_measured_value = i16::from_le_bytes(bytes[2..4].try_into().unwrap());
        let max_measured_value = i16::from_le_bytes(bytes[4..6].try_into().unwrap());
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
    fn test_pressure_measurement() {
        let pressure_measurement =
            PressureMeasurement::new(101.3, 50.0, 200.0, 2).expect("Initialization failed");
        let serialized = pressure_measurement.to_bytes();
        let deserialized =
            PressureMeasurement::from_bytes(&serialized).expect("Deserialization failed");
        assert_eq!(
            pressure_measurement.measured_value,
            deserialized.measured_value
        );
        assert_eq!(
            pressure_measurement.min_measured_value,
            deserialized.min_measured_value
        );
        assert_eq!(
            pressure_measurement.max_measured_value,
            deserialized.max_measured_value
        );
        assert_eq!(pressure_measurement.tolerance, deserialized.tolerance);
    }
}
