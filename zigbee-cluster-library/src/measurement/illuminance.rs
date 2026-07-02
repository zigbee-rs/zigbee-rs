//! Illuminance Measurement and Illuminance Level Sensing Cluster
//!
//! See Section 4.2 & 4.3
//!
//! Provides interfaces to Illuminance measurement and level sensing
//! functionality, including configuration and provision of notifications of
//! wheter the illuminance is within, above or below a target band and
//! illuminance measurements.

/// Illuminance Measurement cluster identifier (ZCL §4.2).
pub const CLUSTER_ID: u16 = 0x0400;

/// Illuminance Level Sensing cluster identifier (ZCL §4.3).
pub const LEVEL_SENSING_CLUSTER_ID: u16 = 0x0401;
