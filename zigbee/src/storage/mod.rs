//! Persistent storage for the information bases.
//!
//! The NIB/AIB live in a RAM mirror; every setter marks its field dirty and
//! signals a change. The application only chooses where the state lives:
//! RAM-only via the plain `init()` functions, or NOR flash via
//! [`init_with_flash`] plus a spawned task running [`FlashStorage::run`] that
//! persists changes as they happen.
//!
//! Frame counters are persisted with headroom so a reboot can never reuse an
//! outgoing counter value: outgoing counters are stored rounded up two
//! `HEADROOM` boundaries ahead, incoming counters rounded down to a `WINDOW`
//! boundary. After a reboot up to `WINDOW` already-seen incoming counter
//! values may be accepted again — the cost of not writing flash on every
//! received frame.
//!
//! The per-IB restore/flush logic lives with the respective information base
//! (`nwk::nib::storage`, `aps::aib::storage`); this module only provides the
//! flash map plumbing.

/// Sink dirty information-base state is flushed into.
pub trait StorageDriver {
    /// Persists all information-base fields modified since the last call.
    async fn flush(&self);
}

/// RAM-only operation: state is lost on reset.
pub struct NoStorage;

impl StorageDriver for NoStorage {
    async fn flush(&self) {}
}

#[cfg(feature = "storage")]
pub(crate) mod flash;

#[cfg(feature = "storage")]
pub(crate) use flash::FlashMap;
#[cfg(feature = "storage")]
pub use flash::FlashStorage;
#[cfg(feature = "storage")]
pub(crate) use flash::Shadow;
#[cfg(feature = "storage")]
pub use flash::init_with_flash;
#[cfg(feature = "storage")]
pub(crate) use flash::round_down;
#[cfg(feature = "storage")]
pub(crate) use flash::round_up;
