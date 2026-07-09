//! Async signaling primitives.
//!
//! Runtime-agnostic: built on [`AtomicWaker`], usable from any executor.

use core::future::poll_fn;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::task::Poll;

use atomic_waker::AtomicWaker;

/// One-shot event flag supporting a single waiter.
#[derive(Default)]
pub struct Event {
    set: AtomicBool,
    waker: AtomicWaker,
}

impl Event {
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
    fn wait_set_leaves_flag() {
        let event = Event::new();
        let mut cx = Context::from_waker(Waker::noop());

        event.signal();
        assert!(pin!(event.wait_set()).poll(&mut cx).is_ready());
        // level semantics: still set
        assert!(pin!(event.wait_set()).poll(&mut cx).is_ready());
    }
}
