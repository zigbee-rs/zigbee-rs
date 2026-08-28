//! ZigBee protocol stack in `no-std`.
//!
//! This is the crate an application depends on: it re-exports the layers of
//! the stack — [`nwk`], [`aps`], [`security`], the cluster library as [`zcl`],
//! the MAC abstraction as [`mac`] and the shared types as [`types`] — and adds
//! the runtime that drives them.
//!
//! The layers below implement the specification and nothing else: the NWK
//! layer owns the keepalive primitives and the rejoin procedure (3.6.10,
//! 3.6.1.4.2), the ZDO owns the Network Manager's rejoin-when-communication-
//! is-lost duty (2.5.2.4), and BDB owns the commissioning procedures. What
//! none of them define is *when* to run those procedures: 3.6.10.3 leaves the
//! keepalive period to the manufacturer, and the order in which a device
//! resumes, rejoins, or re-commissions at startup is an application decision.
//!
//! This crate holds exactly that policy on top of the re-exported layers. An
//! application describes itself once
//! with a [`StackConfig`], builds a [`Stack`] on top of its MAC, and spawns
//! the loops against it:
//!
//! ```ignore
//! let stack = STACK.init(Stack::new(mac, config, handler, storage));
//! spawner.spawn(stack_task(stack));  // stack.run(Delay): commission, then stay on
//! stack.wait_until_joined().await;   // ready to talk to the network
//! ```
//!
//! [`Stack::run`] is the only task the stack needs: it commissions the device,
//! then drives the receive loop, link maintenance and persistence together.
//! Every other task borrows the same [`Stack`] to talk to the network. Drive
//! the parts separately with [`Stack::commission`], [`Stack::rx_loop`] and
//! [`Stack::link_maintenance`] if they need tasks of their own.
#![no_std]

pub mod config;
pub mod stack;

pub use config::DeviceConfig;
pub use config::NetworkConfig;
pub use config::StackConfig;
pub use config::TimingConfig;
pub use stack::Commissioned;
pub use stack::Stack;
pub use stack::StackStopped;
/// The commissioning procedures and their types (BDB).
pub use zigbee_base_device_behavior as bdb;
/// The commissioning procedures the stack runs, reachable via
/// [`Stack::bdb`] for the ones it does not run itself.
pub use zigbee_base_device_behavior::BaseDeviceBehavior;
/// The ZigBee Cluster Library: clusters, attributes and ZCL frames.
pub use zigbee_cluster_library as zcl;
pub use zigbee_core::CurrentPowerMode;
pub use zigbee_core::CurrentPowerSourceLevel;
pub use zigbee_core::LogicalType;
pub use zigbee_core::PowerSource;
/// Application layer descriptors (APL).
pub use zigbee_core::apl;
/// Application support sub-layer (APS): the AIB, bindings and the APSDE-SAP.
pub use zigbee_core::aps;
/// Network layer (NWK): the NIB, the NLME and the network frames.
pub use zigbee_core::nwk;
/// Security services: keys, the trust center and the frame protection.
pub use zigbee_core::security;
/// Persistence of the information bases.
pub use zigbee_core::storage;
/// Flash-backed persistence (feature `storage`): restore the information bases
/// with [`init_with_flash`], then hand the driver to [`Stack::new`].
#[cfg(feature = "storage")]
pub use zigbee_core::storage::FlashStorage;
/// RAM-only persistence: pass it to [`Stack::new`] when state need not survive
/// a reset.
pub use zigbee_core::storage::NoStorage;
#[cfg(feature = "storage")]
pub use zigbee_core::storage::init_with_flash;
/// ZigBee Device Objects — driven by [`Stack`] and [`bdb`].
#[doc(hidden)]
pub use zigbee_core::zdo;
/// ZigBee Device Profile (ZDP) frames.
pub use zigbee_core::zdp;
/// The MAC sub-layer abstraction (`Mlme`) a [`Stack`] is built on.
pub use zigbee_mac as mac;
/// Types shared across the layers: [`types::ShortAddress`],
/// [`types::IeeeAddress`], [`types::StorageVec`].
pub use zigbee_types as types;
