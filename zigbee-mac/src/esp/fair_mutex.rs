//! An async mutex that hands the lock to waiters in FIFO order.
//!
//! `embassy_sync::mutex::Mutex` (and `RwLock`, `Channel`) can starve a
//! low-frequency waiter indefinitely under contention from a much more
//! frequent one — confirmed on real hardware with [`EspMlme`](super::EspMlme)'s
//! shared radio mutex. `embassy_sync::semaphore::FairSemaphore` queues
//! waiters in arrival order instead; this is a thin mutex wrapper around a
//! single-permit one.

use core::cell::UnsafeCell;
use core::ops::Deref;
use core::ops::DerefMut;

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::semaphore::FairSemaphore;
use embassy_sync::semaphore::Semaphore;
use embassy_sync::semaphore::SemaphoreReleaser;
pub use embassy_sync::semaphore::WaitQueueFull;

/// Covers this crate's actual callers: application-level sends, `rx_loop`,
/// `link_maintenance`, one commissioning task.
const MAX_WAITERS: usize = 4;

type Permits<M> = FairSemaphore<M, MAX_WAITERS>;

pub struct FairMutex<M: RawMutex, T> {
    permits: Permits<M>,
    value: UnsafeCell<T>,
}

// Safety: `T` is only reachable through a `FairMutexGuard`, one at a time.
unsafe impl<M: RawMutex + Send, T: Send> Send for FairMutex<M, T> {}
unsafe impl<M: RawMutex + Sync, T: Send> Sync for FairMutex<M, T> {}

impl<M: RawMutex, T> FairMutex<M, T> {
    pub const fn new(value: T) -> Self {
        Self {
            permits: FairSemaphore::new(1),
            value: UnsafeCell::new(value),
        }
    }

    /// Cancel-safe: dropping the returned future removes this task from the
    /// queue. Errors with [`WaitQueueFull`] past `MAX_WAITERS` queued.
    pub async fn lock(&self) -> Result<FairMutexGuard<'_, M, T>, WaitQueueFull> {
        let permit = self.permits.acquire(1).await?;
        Ok(FairMutexGuard {
            mutex: self,
            _permit: permit,
        })
    }
}

#[must_use = "if unused the FairMutex will immediately unlock"]
pub struct FairMutexGuard<'a, M: RawMutex, T> {
    mutex: &'a FairMutex<M, T>,
    _permit: SemaphoreReleaser<'a, Permits<M>>,
}

impl<M: RawMutex, T> Deref for FairMutexGuard<'_, M, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // Safety: the guard holds the mutex's only permit.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<M: RawMutex, T> DerefMut for FairMutexGuard<'_, M, T> {
    fn deref_mut(&mut self) -> &mut T {
        // Safety: see `Deref`.
        unsafe { &mut *self.mutex.value.get() }
    }
}
