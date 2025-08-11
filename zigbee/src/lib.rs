//! Implements the ZigBee protocol stack in `no-std` based on the [ZigBee Specification R23]
//!
//! [ZigBee Specification R23]: https://csa-iot.org/wp-content/uploads/2024/07/docs-05-3474-23-csg-zigbee-specificationR23.1.pdf
//!
//! This crate contains the core network layer and security features.
//! It deals with addressing, keys, trust center, formation and discovery mechanisms.
//!
#![cfg_attr(not(feature = "mock"), no_std)]
//#![deny(clippy::unwrap_used)]
#![deny(clippy::panic, unused_must_use)]
#![warn(
    clippy::missing_safety_doc,
    //missing_docs,
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
    clippy::trivially_copy_pass_by_ref,
    clippy::struct_excessive_bools,
    dead_code,
    unused_imports,
    unused_variables,
    unused_macros,
    clippy::doc_markdown,
    private_interfaces,
)]


pub mod aps;
pub mod apl;
pub mod nwk;
pub mod security;
pub mod zdp;

// ZDO is not directly called by the application — it is controlled by BDB or used internally by the stack.
#[doc(hidden)]
pub mod zdo;

// Device object config
pub use zdo::config::Config;
// Logical type
pub use apl::descriptors::node_descriptor::LogicalType;


// Exposes types and macros only to be within zigbee crates. Not public API.
#[doc(hidden)]
pub mod internal;

