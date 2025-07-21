//! Implements the ZigBee Base Device Behavior in `no-std` based on the [ZigBee Base Device Behavior Specification Rev. 12]
//!
//! [ZigBee Base Device Behavior Specification Rev. 12]: https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf
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


