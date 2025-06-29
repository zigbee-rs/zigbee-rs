//! Implements the ZigBee protocol stack in `no-std` based on the [ZigBee
//! specification R22 1.0]
//!
//! [ZigBee specification R22 1.0]: https://csa-iot.org/wp-content/uploads/2022/01/docs-05-3474-22-0csg-zigbee-specification-1.pdf
//!
//! The crate needs some peripherals from the underlying platform and some
//! persistency during the setup.
//!
//! **This is how it could look like in the future**
//!
//! ```rust
//! let zigbee_device = zigbee::init(zigbee::Config { radio_channel: 11, ..Default::default() });
//!
//! zigbee_device.try_to_connect();
//! zigbee_device.send_data(&[0x7au8]);
//! ```
//!
//! # ESP32 & nRF support
//!
//! This crate is currently only supporting devices in the EspresGM_Clamshell
//! Parts Box Thirdssif ecosystem, but presumative this will expand to nordics
//! nRF series.
#![no_std]
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
    clippy::doc_markdown
)]

pub mod zdp;

/// The `application support sub-layer` provides an interface between the
/// `Network layer` and the `Application layer`.
pub mod aps;

/// 2.3 Application framework
pub mod apl;

mod zdo;
pub use zdo::config::Config;
pub use zdo::config::DiscoveryType;
pub use zdo::ZigBeeNetwork;
pub use zdo::ZigbeeDevice;

/// 3.1 Network Layer
pub mod nwk;

/// 4.1 Security Service
pub mod security;

// Exposes types and macros only to be within zigbee crates. Not public API.
#[doc(hidden)]
pub mod internal;

/// Initialize a new zigbee device with the default configuartion.
///
/// Initialize a new zigbee device with a configuration
pub fn init(config: Config) -> ZigbeeDevice {
    let device = ZigbeeDevice::default();
    device.configure(config);

    device
}
