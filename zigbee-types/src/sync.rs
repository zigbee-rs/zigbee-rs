//! Async signaling primitives.
//!
//! Runtime-agnostic: built on [`AtomicWaker`], usable from any executor.

use core::future::Future;
use core::future::poll_fn;
use core::pin::pin;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::task::Poll;

use atomic_waker::AtomicWaker;
use spin::Mutex;

/// Run `fut` until it completes or `timeout` elapses first.
///
/// Returns `None` when `timeout` won the race.
pub async fn with_timeout<F: Future>(
    fut: F,
    timeout: impl Future<Output = ()>,
) -> Option<F::Output> {
    let mut fut = pin!(fut);
    let mut timeout = pin!(timeout);
    poll_fn(move |cx| {
        if let Poll::Ready(value) = fut.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        match timeout.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

/// Hand control back to the executor once, so another task can make progress.
///
/// Used by yielding `try_lock` loops: a waiter releases the CPU instead of
/// spinning while the holder finishes.
pub async fn yield_now() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}

/// One-shot signal carrying a value to a single waiter.
///
/// The value is produced by the receive path and consumed by the procedure
/// waiting for it; it stays available until taken, so a signal that arrives
/// before anyone waits is not lost.
pub struct Signal<T> {
    value: Mutex<Option<T>>,
    waker: AtomicWaker,
}

impl<T> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Signal<T> {
    /// Creates a signal holding no value.
    pub const fn new() -> Self {
        Self {
            value: Mutex::new(None),
            waker: AtomicWaker::new(),
        }
    }

    /// Store the value and wake the waiter, replacing an unconsumed one.
    pub fn signal(&self, value: T) {
        *self.value.lock() = Some(value);
        self.waker.wake();
    }

    /// Discard a pending, unconsumed value.
    pub fn reset(&self) {
        *self.value.lock() = None;
    }

    /// Take the value without waiting, if one is pending.
    pub fn try_take(&self) -> Option<T> {
        self.value.lock().take()
    }

    /// Whether a value is pending.
    pub fn is_signaled(&self) -> bool {
        self.value.lock().is_some()
    }

    /// Wait until signaled, consuming the value.
    pub async fn wait(&self) -> T {
        poll_fn(|cx| {
            if let Some(value) = self.try_take() {
                return Poll::Ready(value);
            }
            self.waker.register(cx.waker());
            // re-check to close the race with a signal between check and register
            match self.try_take() {
                Some(value) => Poll::Ready(value),
                None => Poll::Pending,
            }
        })
        .await
    }
}

/// One-shot event flag supporting a single waiter.
#[derive(Default)]
pub struct Event {
    set: AtomicBool,
    waker: AtomicWaker,
}

impl Event {
    /// Creates an unset event.
    pub const fn new() -> Self {
        Self {
            set: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }

    /// Set the flag and wake the waiter.
    pub fn signal(&self) {
        self.set.store(true, Ordering::Release);
        self.waker.wake();
    }

    /// Clear a pending, unconsumed signal.
    pub fn reset(&self) {
        self.set.store(false, Ordering::Release);
    }

    /// Whether the flag is currently set.
    pub fn is_set(&self) -> bool {
        self.set.load(Ordering::Acquire)
    }

    /// Wait until signaled, consuming the flag (edge semantics).
    pub async fn wait(&self) {
        poll_fn(|cx| {
            if self.set.swap(false, Ordering::AcqRel) {
                return Poll::Ready(());
            }
            self.waker.register(cx.waker());
            // re-check to close the race with a signal between check and register
            if self.set.swap(false, Ordering::AcqRel) {
                return Poll::Ready(());
            }
            Poll::Pending
        })
        .await;
    }

    /// Wait until signaled, leaving the flag set (level semantics).
    pub async fn wait_set(&self) {
        poll_fn(|cx| {
            if self.set.load(Ordering::Acquire) {
                return Poll::Ready(());
            }
            self.waker.register(cx.waker());
            if self.set.load(Ordering::Acquire) {
                return Poll::Ready(());
            }
            Poll::Pending
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use core::future::Future;
    use core::pin::pin;
    use core::task::Context;
    use core::task::Waker;

    use super::*;

    #[test]
    fn wait_consumes_signal() {
        let event = Event::new();
        let mut cx = Context::from_waker(Waker::noop());

        let mut wait = pin!(event.wait());
        assert!(wait.as_mut().poll(&mut cx).is_pending());

        event.signal();
        assert!(wait.as_mut().poll(&mut cx).is_ready());

        // flag consumed: next wait is pending again
        let mut wait = pin!(event.wait());
        assert!(wait.as_mut().poll(&mut cx).is_pending());
    }

    #[test]
    fn signal_delivers_value_once() {
        let signal = Signal::<u8>::new();
        let mut cx = Context::from_waker(Waker::noop());

        let mut wait = pin!(signal.wait());
        assert!(wait.as_mut().poll(&mut cx).is_pending());

        signal.signal(0x42);
        assert_eq!(wait.as_mut().poll(&mut cx), Poll::Ready(0x42));

        // value consumed: next wait is pending again
        assert!(!signal.is_signaled());
        assert!(pin!(signal.wait()).poll(&mut cx).is_pending());
    }

    #[test]
    fn wait_set_leaves_flag() {
        let event = Event::new();
        let mut cx = Context::from_waker(Waker::noop());

        event.signal();
        assert!(pin!(event.wait_set()).poll(&mut cx).is_ready());
        // level semantics: still set
        assert!(pin!(event.wait_set()).poll(&mut cx).is_ready());
    }
}
