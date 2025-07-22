//! ZigBee Descriptors
//!
//! See Section 2.3.2
//!
//! ZigBee devices describe themselves using descriptor data structures.
//! There are five descriptors: node, node power, simple, complex, and user.

pub mod error;
pub mod node_descriptor;
pub mod node_power_descriptor;
pub mod simple_descriptor;
