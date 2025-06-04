//! 2.3.2  ZigBee Descriptors
//
//! ZigBee devices describe themselves using descriptor data structures.
//! The actual data contained in these descriptors is  defined in the individual
//! device descriptions. There are five descriptors: node, node power, simple,
//! complex, and user.

pub mod error;
pub mod node_descriptor;
pub mod node_power_descriptor;
pub mod simple_descriptor;
