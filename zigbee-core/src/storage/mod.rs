//! Persistent storage for the information bases.
//!
//! The NIB/AIB live in a RAM mirror; every setter marks its field dirty and
//! signals a change. The application only chooses where the state lives:
//! RAM-only via the plain `init()` functions, or NOR flash via
//! [`init_with_flash`] plus a spawned task running [`FlashStorage::run`] that
//! persists changes as they happen.
//!
//! Frame counters are persisted so that a reboot can never reuse an outgoing
//! counter value (4.3.4). Outgoing counters are stored `HEADROOM` ahead of the
//! live value because flushing is asynchronous; they are rewritten on every
//! flush, so a transmitted frame costs a flash write. Incoming counters are
//! stored exactly, so a received frame costs one too — in exchange there is no
//! replay window to re-accept after a reboot. They live in a flat NIB table
//! keyed by (key sequence number, sender) rather than nested in each security
//! material descriptor, so one sender advancing rewrites one row.
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
pub(crate) use flash::HEADROOM;
#[cfg(feature = "storage")]
pub(crate) use flash::PersistentIb;
#[cfg(feature = "storage")]
pub use flash::init_with_flash;
