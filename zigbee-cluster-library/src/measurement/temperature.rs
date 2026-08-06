//! Temperature Measurement Cluster
//!
//! See Section 4.4
//!
//! Provides an interface to temperature measurement functionality, including
//! configuration and provision of notifications of temperature measurements.
use heapless::Vec;

/// Cluster identifier (ZCL 4.4).
pub const CLUSTER_ID: u16 = 0x0402;

/// Attribute identifiers (ZCL 4.4.2.2.1).
pub mod attribute {
    /// `MeasuredValue` (`Int16`, hundredths of a degree Celsius).
    pub const MEASURED_VALUE: u16 = 0x0000;
    /// `MinMeasuredValue` (`Int16`).
    pub const MIN_MEASURED_VALUE: u16 = 0x0001;
    /// `MaxMeasuredValue` (`Int16`).
    pub const MAX_MEASURED_VALUE: u16 = 0x0002;
    /// `Tolerance` (`Uint16`).
    pub const TOLERANCE: u16 = 0x0003;
}

/// Temperature Measurement Attribute Set
///
/// See Section 4.4.2.2.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperatureMeasurement {
    Measured(i16),
    MinMeasuredValue(i16),
    MaxMeasuredValue(i16),
    Tolerance(u16),
    Unknown,
}

impl TemperatureMeasurement {
    // TODO: encode variant into its attribute id + value bytes
    pub fn to_bytes(&self) -> Vec<u8, 8> {
        Vec::new()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 8 {
            return Err("Invalid byte slice length");
        }

        // TODO: decode attribute id + value bytes into a variant
        Err("TODO")
    }

    pub fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        let bytes: Vec<u8, 8> = src.into_iter().collect();
        Self::from_bytes(&bytes).ok()
    }
}
