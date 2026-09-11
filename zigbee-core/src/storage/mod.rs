//! Persistent storage for the information bases.
//!
//! The NIB/AIB live in a RAM mirror; every setter marks its field dirty and
//! signals a change. The application only chooses where the state lives:
//! RAM-only via the plain `init()` functions, or NOR flash via
//! [`FlashStorage::new`] plus a spawned task running [`FlashStorage::run`] that
//! persists changes as they happen.
//!
//! Frame counters are persisted so that a reboot can never reuse an outgoing
//! counter value (4.3.4). What is stored is a quantized bound, not the live
//! value: outgoing counters are rounded up two `HEADROOM` boundaries ahead,
//! incoming counters rounded down to a `WINDOW` boundary. A counter moving
//! inside its current step changes nothing that is stored, so the mutation
//! sites advance it without marking it dirty and only mark on a crossing —
//! that is what keeps the flash write rate far below the frame rate. After a
//! reboot up to `WINDOW` already-seen incoming counter values may be accepted
//! again, the cost of not writing flash on every received frame.
//!
//! Table fields are stored one map item per row plus a length record, and only
//! the rows a caller actually touched are rewritten. Mutating a table through
//! `update_<field>` conservatively marks every row; the `<field>_mut` handle
//! marks just the rows it changes. A shrunk table leaves its surplus rows in
//! flash — the length record bounds what is read back, which avoids needing
//! erasable items.
//!
//! How a field is encoded is IB-specific and lives with the respective
//! information base (`nwk::nib::storage`, `aps::aib::storage`) as a
//! `PersistentIb` impl; this module only provides the flash map plumbing.

// outgoing frame counters are stored this far ahead of the live value, so a
// power cut can never hand out a counter that was already transmitted
pub(crate) const HEADROOM: u32 = 1024;
// incoming frame counters are stored rounded down to this granularity
const WINDOW: u32 = 1024;

// next counter value a rebooted device may use; two boundaries ahead so the
// stored bound is refreshed a full HEADROOM before it could be reached
pub(crate) const fn round_up(counter: u32) -> u32 {
    (counter / HEADROOM)
        .saturating_add(2)
        .saturating_mul(HEADROOM)
}

pub(crate) const fn round_down(counter: u32) -> u32 {
    (counter / WINDOW) * WINDOW
}

/// Sink dirty information-base state is flushed into.
pub trait StorageDriver {
    /// Persists all information-base fields modified since the last call.
    async fn flush(&self);

    /// Persists information-base changes as they happen, until cancelled.
    ///
    /// Drivers that write in the background implement this; the default never
    /// completes, so a caller can drive any driver the same way.
    async fn run(&self) {
        core::future::pending::<()>().await;
    }
}

/// Lets a driver be shared: the stack can hold a reference to one the
/// application also flushes itself.
impl<T: StorageDriver> StorageDriver for &T {
    async fn flush(&self) {
        (**self).flush().await;
    }

    async fn run(&self) {
        (**self).run().await;
    }
}

/// RAM-only operation: state is lost on reset.
pub struct NoStorage;

impl StorageDriver for NoStorage {
    async fn flush(&self) {}
}

#[cfg(feature = "storage")]
pub(crate) mod flash;

#[cfg(feature = "storage")]
pub use flash::FlashStorage;
#[cfg(feature = "storage")]
pub(crate) use flash::PersistentIb;
