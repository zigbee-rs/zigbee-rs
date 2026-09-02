//! Relative Humidity Measurement
//!
//! See Section 4.7
//!
//! Provides an interface to relative humidity measurement functionality,
//! including configuration and provision of notifications of relative humidity
//! measurements.
//!
//! The specification defines this cluster as one of three sharing the Water
//! Content Measurement attribute set, so the definitions live in
//! [`super::water_content`] and are re-exported here.

pub use super::water_content::FULL_SCALE;
pub use super::water_content::UNKNOWN;
pub use super::water_content::WaterContent;
pub use super::water_content::attribute;

/// Cluster identifier (ZCL 4.7).
pub const CLUSTER_ID: u16 = super::water_content::RELATIVE_HUMIDITY_CLUSTER_ID;
