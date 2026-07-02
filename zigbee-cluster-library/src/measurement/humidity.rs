//! Relative Humidity Measurement
//!
//! See Section 4.7
//!
//! Provides an interface to relative humidity measurement functionality,
//! including configuration and provision of notifications of relative humidity
//! measurements.

/// Cluster identifier (ZCL §4.7).
pub const CLUSTER_ID: u16 = 0x0405;

/// Attribute identifiers (ZCL §4.7.2.2.1).
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
