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
//! incoming counters rounded down to an `RX_WINDOW` boundary. A counter moving
//! inside its current step changes nothing that is stored, so the mutation
//! sites advance it without marking it dirty and only mark on a crossing —
//! that is what keeps the flash write rate far below the frame rate.
//!
//! # Accepted replay window
//!
//! Rounding incoming counters *down* is the one place this trades security for
//! flash life, and it is deliberate. After an unclean reset the restored value
//! is below the highest counter actually accepted, so up to `RX_WINDOW` counter
//! values from that sender pass the anti-replay check a second time (4.3.1.2).
//! An attacker within radio range who recorded those frames and can provoke or
//! await a reset — a power cut, a brownout, a watchdog, a battery change — can
//! replay them, without holding any key. It costs an APS command its
//! single-delivery guarantee, so a recorded lock, alarm or on/off command can
//! be made to act a second time.
//!
//! Writing every accepted counter instead (`RX_WINDOW` of 1) closes it and
//! costs one sector erase per 163 received frames, which wears a 16 KiB region
//! out in months on a router. The window is the compromise, and the constant
//! is the dial.
//!
//! [`StorageDriver::shutdown`] removes the window entirely for every reset the
//! application sees coming, by writing the counters exactly; `RX_WINDOW` bounds
//! only what an unplanned reset gives up. Applications that cannot accept the
//! remainder should lower it and budget for the wear.
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

// SECURITY: incoming frame counters are stored rounded down to this
// granularity, which accepts a bounded replay window after an unclean reset —
// see the module documentation. Deliberate, and the value is the dial: it
// trades that window against flash wear one-for-one. Measured over 200k
// accepted frames into a 16 KiB region, a window of 256 costs 2 sector erases
// where 32 costs 37 and 1 costs 1227; at 1 a router sustaining 10 frames/s
// wears the region out in about three months
const RX_WINDOW: u32 = 256;

// next counter value a rebooted device may use; two boundaries ahead so the
// stored bound is refreshed a full HEADROOM before it could be reached
pub(crate) const fn round_up(counter: u32) -> u32 {
    (counter / HEADROOM)
        .saturating_add(2)
        .saturating_mul(HEADROOM)
}

pub(crate) const fn round_down(counter: u32) -> u32 {
    (counter / RX_WINDOW) * RX_WINDOW
}

/// How much of a frame counter reaches flash.
#[cfg(feature = "storage")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CounterPrecision {
    /// Quantized to a window boundary, which keeps the write rate far below
    /// the frame rate.
    Quantized,
    /// Incoming counters written exactly, so a planned reset resumes
    /// anti-replay where it left off. Outgoing counters keep their headroom
    /// either way: the device may still transmit after the flush.
    Exact,
}

/// Sink dirty information-base state is flushed into.
pub trait StorageDriver {
    /// Persists all information-base fields modified since the last call.
    async fn flush(&self);

    /// Persists everything, writing incoming frame counters exactly.
    ///
    /// Call before a planned reset, a deep sleep or a firmware update: the
    /// next boot then rejects every frame counter it had already accepted,
    /// instead of the window below it that the quantized bounds give up. An
    /// unplanned reset still falls back to those bounds.
    async fn shutdown(&self) {
        self.flush().await;
    }

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

    async fn shutdown(&self) {
        (**self).shutdown().await;
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
