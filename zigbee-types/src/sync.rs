//! Async signaling primitives.
//!
//! Runtime-agnostic: built on [`AtomicWaker`], usable from any executor.

use core::future::Future;
use core::future::poll_fn;
use core::pin::pin;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::AtomicU32;
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
            // re-check to close the race with a signal between check and
            // register
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
            // re-check to close the race with a signal between check and
            // register
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

/// Lock-free set of up to 64 bits, e.g. the modified fields of an
/// information base.
///
/// Split into two 32-bit halves because riscv32 targets (ESP32-C6/H2) have
/// no 64-bit atomics.
pub struct BitSet64 {
    lo: AtomicU32,
    hi: AtomicU32,
}

impl BitSet64 {
    /// Creates an empty set.
    pub const fn new() -> Self {
        Self {
            lo: AtomicU32::new(0),
            hi: AtomicU32::new(0),
        }
    }

    /// Adds a bit; `index` must be below 64.
    pub fn set(&self, index: u8) {
        debug_assert!(index < 64, "bit index out of range");
        let (half, bit) = if index < 32 {
            (&self.lo, index)
        } else {
            (&self.hi, index - 32)
        };
        half.fetch_or(1 << bit, Ordering::Release);
    }

    /// Removes a bit; `index` must be below 64.
    pub fn clear(&self, index: u8) {
        debug_assert!(index < 64, "bit index out of range");
        let (half, bit) = if index < 32 {
            (&self.lo, index)
        } else {
            (&self.hi, index - 32)
        };
        half.fetch_and(!(1 << bit), Ordering::Release);
    }

    /// Returns all bits and empties the set.
    ///
    /// Bits added between the two half-swaps stay set and are returned by the
    /// next call.
    pub fn take(&self) -> u64 {
        let lo = self.lo.swap(0, Ordering::AcqRel);
        let hi = self.hi.swap(0, Ordering::AcqRel);
        u64::from(lo) | (u64::from(hi) << 32)
    }
}

impl Default for BitSet64 {
    fn default() -> Self {
        Self::new()
    }
}

/// Interior-mutable storage of a single information-base field.
///
/// Implemented by `spin::RwLock` for compound fields and by the plain
/// atomics for primitives, so an information base reads and writes every
/// field the same way regardless of how it is stored.
pub trait IbCell<T> {
    /// What a read hands back: a guard for locked fields, a copy for atomics.
    type Ref<'a>
    where
        Self: 'a;

    fn new(value: T) -> Self;

    fn get(&self) -> Self::Ref<'_>;

    fn set(&self, value: T);

    /// Applies `f` to the stored value.
    ///
    /// Atomic fields load, apply and store rather than doing a real
    /// read-modify-write. That is sound only because the stack is
    /// cooperatively scheduled and `f` cannot await: a concurrent writer
    /// would lose an update, and this path carries the outgoing frame counter,
    /// where a lost increment means a reused CCM* nonce. Never call it from
    /// an interrupt.
    fn update(&self, f: impl FnOnce(&mut T));

    /// Returns an owned copy of the stored value.
    fn get_owned(&self) -> T
    where
        T: Clone;
}

impl<T> IbCell<T> for spin::RwLock<T> {
    type Ref<'a>
        = spin::RwLockReadGuard<'a, T>
    where
        T: 'a;

    fn new(value: T) -> Self {
        spin::RwLock::new(value)
    }

    fn get(&self) -> Self::Ref<'_> {
        self.read()
    }

    fn set(&self, value: T) {
        *self.write() = value;
    }

    fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut *self.write());
    }

    fn get_owned(&self) -> T
    where
        T: Clone,
    {
        T::clone(&*self.read())
    }
}

/// A primitive that an information base can keep in a plain atomic instead
/// of behind a lock.
pub trait AtomicCell: Copy {
    /// The atomic holding it.
    type Cell: IbCell<Self>;
}

macro_rules! atomic_cell {
    ($($ty:ty => $cell:ty,)+) => {
        $(
            impl AtomicCell for $ty {
                type Cell = $cell;
            }

            impl IbCell<$ty> for $cell {
                type Ref<'a> = $ty;

                fn new(value: $ty) -> Self {
                    <$cell>::new(value)
                }

                fn get(&self) -> $ty {
                    self.load(Ordering::Acquire)
                }

                fn set(&self, value: $ty) {
                    self.store(value, Ordering::Release);
                }

                fn update(&self, f: impl FnOnce(&mut $ty)) {
                    let mut value = self.load(Ordering::Acquire);
                    f(&mut value);
                    self.store(value, Ordering::Release);
                }

                fn get_owned(&self) -> $ty {
                    self.load(Ordering::Acquire)
                }
            }
        )+
    };
}

atomic_cell! {
    bool => AtomicBool,
    u8 => AtomicU8,
    u16 => AtomicU16,
    u32 => AtomicU32,
}

/// A collection an information base stores entry by entry.
///
/// Lets [`TableMut`] drive any table without naming its capacity, so the
/// generated information bases stay free of const generics.
pub trait Table {
    /// What one row holds.
    type Entry;

    /// Rows the table can ever hold.
    const CAPACITY: usize;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Rows the table can ever hold.
    fn capacity(&self) -> usize;

    fn get(&self, index: usize) -> Option<&Self::Entry>;

    fn get_mut(&mut self, index: usize) -> Option<&mut Self::Entry>;

    /// Appends `entry`, returning it back when the table is full.
    fn push(&mut self, entry: Self::Entry) -> Result<(), Self::Entry>;

    fn remove(&mut self, index: usize);

    fn clear(&mut self);
}

impl<T, const N: usize> Table for crate::StorageVec<T, N> {
    type Entry = T;

    const CAPACITY: usize = N;

    fn len(&self) -> usize {
        self.0.len()
    }

    fn capacity(&self) -> usize {
        N
    }

    fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.0.get_mut(index)
    }

    fn push(&mut self, entry: T) -> Result<(), T> {
        self.0.push(entry)
    }

    fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// Cell holding an information-base table together with the set of entries
/// changed since the last flush.
pub struct TableCell<V> {
    entries: spin::RwLock<V>,
    dirty: BitSet64,
    // the length record only needs rewriting when rows are added or removed,
    // not when one is updated in place
    len_dirty: AtomicBool,
}

impl<V: Table> TableCell<V> {
    /// Borrows the table for entry-level mutation, recording exactly which
    /// rows change.
    pub fn table_mut<'a>(&'a self, signal: &'a Event) -> TableMut<'a, V> {
        TableMut::new(self.entries.write(), &self.dirty, &self.len_dirty, signal)
    }

    /// Returns and clears the set of entries changed since the last call.
    pub fn take_dirty_entries(&self) -> u64 {
        self.dirty.take()
    }

    /// Returns and clears whether the number of entries changed since the
    /// last call.
    pub fn take_len_dirty(&self) -> bool {
        self.len_dirty.swap(false, Ordering::Acquire)
    }
}

impl<V: Table> IbCell<V> for TableCell<V> {
    type Ref<'a>
        = spin::RwLockReadGuard<'a, V>
    where
        V: 'a;

    fn new(value: V) -> Self {
        Self {
            entries: spin::RwLock::new(value),
            dirty: BitSet64::new(),
            len_dirty: AtomicBool::new(false),
        }
    }

    fn get(&self) -> Self::Ref<'_> {
        self.entries.read()
    }

    // replaces the whole table: the row count can move, but the rows
    // themselves are not marked, since restore and reset use this
    fn set(&self, value: V) {
        *self.entries.write() = value;
        self.len_dirty.store(true, Ordering::Release);
    }

    fn update(&self, f: impl FnOnce(&mut V)) {
        let mut entries = self.entries.write();
        f(&mut entries);
        // the closure has the whole table, so any row may have moved
        for index in 0..entries.capacity().min(TRACKED_ENTRIES) {
            self.dirty.set(index as u8);
        }
        self.len_dirty.store(true, Ordering::Release);
    }

    fn get_owned(&self) -> V
    where
        V: Clone,
    {
        V::clone(&*self.entries.read())
    }
}

/// Entries a table tracks individually; the dirty set is one `BitSet64`.
///
/// A table may not hold more rows than this; the information bases assert it
/// at compile time.
pub const TRACKED_ENTRIES: usize = 64;

/// Write handle to an information-base table that records exactly which
/// entries changed.
///
/// Every mutation goes through a method that marks the affected rows, so the
/// storage layer can persist just those rows instead of the whole table.
/// Entries beyond the 64th are not tracked individually and always count as
/// changed.
pub struct TableMut<'a, V: Table> {
    entries: spin::RwLockWriteGuard<'a, V>,
    dirty: &'a BitSet64,
    len_dirty: &'a AtomicBool,
    signal: &'a Event,
}

impl<'a, V: Table> TableMut<'a, V> {
    #[doc(hidden)]
    pub fn new(
        entries: spin::RwLockWriteGuard<'a, V>,
        dirty: &'a BitSet64,
        len_dirty: &'a AtomicBool,
        signal: &'a Event,
    ) -> Self {
        Self {
            entries,
            dirty,
            len_dirty,
            signal,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<&V::Entry> {
        self.entries.get(index)
    }

    /// Index of the first entry matching `f`.
    pub fn position(&self, f: impl Fn(&V::Entry) -> bool) -> Option<usize> {
        (0..self.len()).find(|index| self.entries.get(*index).is_some_and(&f))
    }

    /// Applies `f` to one entry and marks it for persistence.
    ///
    /// Does nothing when `index` is out of bounds.
    pub fn update(&mut self, index: usize, f: impl FnOnce(&mut V::Entry)) {
        let Some(entry) = self.entries.get_mut(index) else {
            return;
        };
        f(entry);
        self.mark(index);
    }

    /// Applies `f` to one entry without marking it for persistence.
    ///
    /// For changes the stored image does not reflect, such as a frame counter
    /// moving inside the window its stored value is rounded to. The caller is
    /// responsible for using [`Self::update`] once the stored image would
    /// actually change.
    pub fn update_quiet(&mut self, index: usize, f: impl FnOnce(&mut V::Entry)) {
        if let Some(entry) = self.entries.get_mut(index) {
            f(entry);
        }
    }

    /// Appends `entry`, returning its index.
    pub fn push(&mut self, entry: V::Entry) -> Result<usize, V::Entry> {
        let index = self.entries.len();
        self.entries.push(entry)?;
        self.mark_len();
        self.mark(index);
        Ok(index)
    }

    /// Removes one entry; the rows after it shift down and are marked too.
    pub fn remove(&mut self, index: usize) {
        let previous_len = self.entries.len();
        self.entries.remove(index);
        self.mark_len();
        self.mark_range(index, previous_len);
    }

    pub fn clear(&mut self) {
        let previous_len = self.entries.len();
        self.entries.clear();
        self.mark_len();
        self.mark_range(0, previous_len);
    }

    fn mark_len(&self) {
        self.len_dirty.store(true, Ordering::Release);
    }

    fn mark(&self, index: usize) {
        // entries past the tracked range cannot be addressed individually, so
        // treat any change to them as a change to the whole table
        if index < TRACKED_ENTRIES {
            self.dirty.set(index as u8);
        } else {
            self.mark_range(0, TRACKED_ENTRIES);
        }
        self.signal.signal();
    }

    fn mark_range(&self, from: usize, to: usize) {
        for index in from..to.min(TRACKED_ENTRIES) {
            self.dirty.set(index as u8);
        }
        self.signal.signal();
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
