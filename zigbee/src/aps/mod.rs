//! Application Support Sub-Layer
//!
//! See Section 2.2
//!
//! The application support sub-layer provides an interface between the
//! `Network layer` and the `Application layer`.

pub(crate) mod error;
pub mod types;

/// Maximum number of retries after a transmission failure (2.2.7.1).
pub(crate) const APSC_MAX_FRAME_RETRIES: u8 = 3;

/// Time to wait for an APS acknowledgement (2.2.7.1):
/// `0.05 * (2 * nwkcMaxDepth) + 0.1 s` at a maximum depth of 15.
pub(crate) const APSC_ACK_WAIT_DURATION_MS: u32 = 1600;

/// Entries kept for duplicate rejection (2.2.8.4.2); the spec requires at
/// least `apscMinDuplicateRejectionTableSize` = 1.
pub(crate) const APS_DUPLICATE_REJECTION_TABLE_SIZE: usize = 8;

/// Concurrently outstanding acknowledged transmissions.
pub(crate) const APS_MAX_PENDING_ACKS: usize = 2;

/// The APS data entity provides the data transmission service between two or
/// more application entities located on the same network.
pub mod apsde;

pub mod aib;
/// The APS management entity provides a variety of services to application
/// objects including security services and binding of devices.
/// It also maintains a database of managed objects, known as the APS
/// information base (AIB).
pub mod apsme;
/// Binding table (2.2.8.2.1).
pub mod binding;
/// APS frame formats (2.2.5).
pub mod frame;
pub mod security;
