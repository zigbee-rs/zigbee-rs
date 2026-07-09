use core::future::poll_fn;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::task::Poll;
use core::task::Waker;

use embedded_storage::ReadStorage;
use embedded_storage::Storage;

/// Single-waiter change notification.
///
/// Producers call [`Self::notify`] from sync code; one consumer task awaits
/// [`Self::changed`]. Notifications coalesce: many notifies before the
/// consumer wakes yield a single wake-up.
pub struct DirtySignal {
    dirty: AtomicBool,
    waker: spin::Mutex<Option<Waker>>,
}

impl DirtySignal {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            dirty: AtomicBool::new(false),
            waker: spin::Mutex::new(None),
        }
    }

    /// Marks the signal and wakes the waiting task, if any.
    pub fn notify(&self) {
        self.dirty.store(true, Ordering::Release);
        let waker = self.waker.lock().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Waits until notified, consuming the notification.
    ///
    /// Only one task may wait at a time; a second concurrent waiter replaces
    /// the first one's wake-up registration.
    pub async fn changed(&self) {
        poll_fn(|cx| {
            if self.dirty.swap(false, Ordering::AcqRel) {
                return Poll::Ready(());
            }
            *self.waker.lock() = Some(cx.waker().clone());
            // re-check to close the notify-before-registration race
            if self.dirty.swap(false, Ordering::AcqRel) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

impl Default for DirtySignal {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InMemoryStorage<const N: usize> {
    buf: [u8; N],
}

impl<const N: usize> Default for InMemoryStorage<N> {
    fn default() -> Self {
        Self { buf: [0u8; N] }
    }
}

impl<const N: usize> ReadStorage for InMemoryStorage<N> {
    type Error = ();

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let size = offset + bytes.len();
        bytes.copy_from_slice(&self.buf[offset..size]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> Storage for InMemoryStorage<N> {
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let size = offset + bytes.len();
        self.buf[offset..size].copy_from_slice(bytes);
        Ok(())
    }
}
