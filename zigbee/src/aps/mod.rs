//! Application Support Sub-Layer
//!
//! See Section 2.2
//!
//! The application support sub-layer provides an interface between the
//! `Network layer` and the `Application layer`.

pub(crate) mod error;
pub(crate) mod types;

/// The APS data entity provides the data transmission service between two or
/// more application entities located on the same network.
pub mod apsde;

pub mod aib;
pub mod apdu;
/// The APS management entity provides a variety of services to application
/// objects including security services and binding of devices.
/// It also maintains a database of managed objects, known as the APS
/// information base (AIB).
pub mod apsme;
mod binding;
