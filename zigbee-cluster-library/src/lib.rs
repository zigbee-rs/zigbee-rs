//! Implements the `Zigbee` Cluster Library in `no-std` based on the [Zigbee
//! Cluster Library Rev 8]
//!
//! This crate defines application-level behaviors, like reading attributes,
//! reporting, and commands. It contains standard clusters like Temperature
//! Measurement, Basic Identify, etc.
//!
//! [Zigbee Cluster Library Rev 8]: https://csa-iot.org/wp-content/uploads/2022/01/07-5123-08-Zigbee-Cluster-Library-1.pdf
#![no_std]
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
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_raw_string_hashes,
    clippy::blocks_in_conditions,
    clippy::missing_const_for_fn,
    clippy::future_not_send,
    clippy::ignored_unit_patterns,
    clippy::redundant_pub_crate,
    clippy::large_enum_variant,
    clippy::derive_partial_eq_without_eq,
    clippy::too_long_first_doc_paragraph,
    async_fn_in_trait
)]

macro_rules! bad_input {
    ($msg:expr) => {
        byte::Error::BadInput { err: $msg }
    };
}

pub mod clusters;
pub mod frame;
pub mod profile;
pub mod reporting;
pub mod sender;
pub mod server;
pub mod types;
