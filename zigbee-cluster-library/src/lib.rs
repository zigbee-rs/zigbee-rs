//! Implements the ZigBee Cluster Library in `no-std` based on the [ZigBee
//! Cluster Library specification R6 1.0]
//!
//! [ZigBee Cluster Library specification R6 1.0]: https://zigbeealliance.org/wp-content/uploads/2019/12/07-5123-06-zigbee-cluster-library-specification.pdf
#![no_std]
//#![deny(clippy::unwrap_used)]
// #![deny(clippy::panic, unused_must_use)]
#![warn(
    // missing_docs,
    // unreachable_pub,
    clippy::pedantic,
    clippy::nursery,
    clippy::tests_outside_test_module,
    unused_crate_dependencies,
    unused_qualifications,
    single_use_lifetimes,
    non_ascii_idents
)]
#![allow(
    clippy::missing_errors_doc,
    // clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_raw_string_hashes,
    clippy::blocks_in_conditions,
    clippy::missing_const_for_fn,
    clippy::future_not_send,
    clippy::ignored_unit_patterns,
    dead_code,
)]

pub(crate) mod common;

/// General ZCL Frame
pub(crate) mod frame;
pub(crate) mod payload;

pub(crate) mod header;

// Chapter 4
pub mod measurement;
// Chapter 5
pub mod lighting;
// Chapter 6
pub mod hvac;
// Chapter 10
pub mod energy;

