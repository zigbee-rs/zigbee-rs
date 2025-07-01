//!
//! 4.4 Temperature Measurement Cluster
use heapless::Vec;

use crate::common::parse::PackBytes;

/// TODO: add useful information
pub enum TemperatureMeasurement {
    Measured(i16),
    MinMeasuredValue(i16),
    MaxMeasuredValue(i16),
    Tolerance(u16),
    Unknown,
}

impl PackBytes for TemperatureMeasurement {
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        let b = src.into_iter().next()?;

        match b {
            0x0000 => Some(Self::Measured(0)),
            0x0001 => Some(Self::MinMeasuredValue(0)),
            0x0002 => Some(Self::MaxMeasuredValue(0)),
            0x0003 => Some(Self::Tolerance(0)),
            // TODO: handle unknown u16
            // 0x8000 => Some(Self::Unknown),
            _ => None
        }
    }
}


impl TemperatureMeasurement {
    pub fn to_bytes(&self) -> Vec<u8, 8> {
        let mut bytes = Vec::new();
        // bytes
        //     .extend_from_slice(&self.to_bytes())
        //     .unwrap();

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 8 {
            return Err("Invalid byte slice length");
        }

        Err("TODO")
        // let measured_value = i16::from_le_bytes(bytes[0..2].try_into().unwrap());
        // let min_measured_value = i16::from_le_bytes(bytes[2..4].try_into().unwrap());
        // let max_measured_value = i16::from_le_bytes(bytes[4..6].try_into().unwrap());
        // let tolerance = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        //
        // Ok(Self {
        //     measured_value,
        //     min_measured_value,
        //     max_measured_value,
        //     tolerance,
        // })
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
    fn test_binary_as_measured_value() {
        // given
        let data = [0x0b, 0x8a];

        // when
        // let temperature = TemperatureMeasurement::unpack_from_slice(&data).unwrap();

        // then
        // assert_eq!(temperature, 29.54);
    }

    #[test]
    fn test_serialize_temperature_measurement() {
        // given
        let temp_measurement = TemperatureMeasurement::Measured(2345);

        // when
        let serialized = temp_measurement.to_bytes();

        // then
        assert_eq!(serialized, [0x00, 0x00]);
    }

    #[test]
    fn test_deserialize_temperature_measurement() {
        // given
        let serialized = [0x00, 0x00]; 

        // when
        let deserialized = TemperatureMeasurement::from_bytes(&serialized).expect("Deserialization failed");

        // then
        // assert_eq!(serialized, TemperatureMeasurement::Measured(2345));
    }
}
