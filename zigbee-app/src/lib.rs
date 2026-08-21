//! Application-facing runtime for the zigbee stack.
//!
//! The layers below implement the specification and nothing else: the NWK
//! layer owns the keepalive primitives and the rejoin procedure (3.6.10,
//! 3.6.1.4.2), the ZDO owns the Network Manager's rejoin-when-communication-
//! is-lost duty (2.5.2.4), and BDB owns the commissioning procedures. What
//! none of them define is *when* to run those procedures: 3.6.10.3 leaves the
//! keepalive period to the manufacturer, and the order in which a device
//! resumes, rejoins, or re-commissions at startup is an application decision.
//!
//! This crate holds exactly that policy. An application describes itself once
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
/// Flash-backed persistence (feature `storage`): restore the information bases
/// with [`init_with_flash`], then hand the driver to [`Stack::new`].
#[cfg(feature = "storage")]
pub use zigbee::storage::FlashStorage;
/// RAM-only persistence: pass it to [`Stack::new`] when state need not survive
/// a reset.
pub use zigbee::storage::NoStorage;
#[cfg(feature = "storage")]
pub use zigbee::storage::init_with_flash;
/// The commissioning procedures the stack runs, reachable via
/// [`Stack::bdb`] for the ones it does not run itself.
pub use zigbee_base_device_behavior::BaseDeviceBehavior;
