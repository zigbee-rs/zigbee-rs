//! Measurement and Sensing
//!
//! See Section 4
//!
//! Generic measurement and sensing interfaces

// 4.2 | 4.3
pub mod illuminance;

// 4.4
pub mod temperature;

// 4.5
pub mod pressure;

// 4.6
pub mod flow;

// 4.7
pub mod water_content;
// 4.7, one of the three Water Content Measurement clusters
pub mod humidity;

// 4.8
pub mod occupancy;

// 4.9
pub mod electrical;

// 4.10
pub mod electrical_conductivity;

// 4.11
pub mod ph;

// 4.12
pub mod wind_speed;
