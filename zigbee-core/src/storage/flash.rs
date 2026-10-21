use core::future::poll_fn;
use core::ops::Range;
use core::pin::pin;
use core::task::Poll;

use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::cache::KeyPointerCache;
use sequential_storage::map::MapConfig;
use sequential_storage::map::MapStorage;
use sequential_storage::map::SerializationError;
use sequential_storage::map::Value;
use zigbee_types::sync::TRACKED_ENTRIES;
use zigbee_types::sync::yield_now;

use super::CounterPrecision;
use super::HEADROOM;
use super::StorageDriver;
use super::round_up;
use crate::aps::aib;
use crate::nwk::nib;

// sequential-storage work buffer: largest item + item overhead. Tables are
// stored one entry per item, so a table never sizes this buffer as a whole
const SCRATCH: usize = {
    let a = nib::NibId::MAX_FIELD_SIZE;
    let b = aib::AibId::MAX_FIELD_SIZE;
    let c = nib::NibId::MAX_ENTRY_SIZE;
    let d = aib::AibId::MAX_ENTRY_SIZE;
    let mut max = a;
    if b > max {
        max = b;
    }
    if c > max {
        max = c;
    }
    if d > max {
        max = d;
    }
    max + 64
};
// every map item must fit into one flash sector (4 KiB on esp32)
const _: () = assert!(SCRATCH <= 4096);

// Pages of the region the cache tracks. A larger region still works, it just
// keeps fewer of its page states cached
const CACHED_PAGES: usize = 16;

// Every key a full device can hold, so garbage collection never falls back to
// a scan. `migrate_items` asks whether each item on the page it is recycling
// is still the newest copy of its key; uncached that is a walk of the whole
// map per item, which is quadratic in the number of items and is what made a
// single store take tens of seconds once tables were stored per row
const CACHED_KEYS: usize = nib::NibId::MAX_ITEMS + aib::AibId::MAX_ITEMS;

// the cache starts empty on a cold boot and `fetch_all_items` does not fill
// it, so the first collection after a reboot still scans. Restoring always
// stores a counter bound, which can be the write that triggers it, and that
// boot then stalls for as long as the scan takes. Enlarging the region makes
// it worse, not better: the scan grows with the number of items it holds.
// Filling the cache from the restore pass would settle it, but that needs
// `MapItemIter` to report where each item sits
type Cache = KeyPointerCache<CACHED_PAGES, u32, CACHED_KEYS>;

// entry index reserved for a table's length record
const LEN_INDEX: u16 = 0xffff;

// written over a row that a shrunk table no longer holds
const EMPTY_ROW: &[u8] = &[];

// map key: information base, field, then row within the field
const fn item_key(tag: u8, field: u8, index: u16) -> u32 {
    ((tag as u32) << 24) | ((field as u32) << 16) | index as u32
}

// table fields either information base persists; sizes the restore
// bookkeeping
const MAX_TABLES: usize = if nib::NibId::TABLES > aib::AibId::TABLES {
    nib::NibId::TABLES
} else {
    aib::AibId::TABLES
};

// what the restore pass found in the map for one table field
struct RestoredTable {
    field: u8,
    // bit per row index the map holds
    rows: u64,
    // rows the length record claims
    len: u16,
}

impl RestoredTable {
    fn set_row(&mut self, index: u16, held: bool) {
        if index >= 64 {
            return;
        }
        if held {
            self.rows |= 1 << index;
        } else {
            self.rows &= !(1 << index);
        }
    }

    // rows a table keeps: what the length record claims, cut short at the
    // first row the map does not hold. A power loss while a shrink empties its
    // rows leaves the record claiming more than the table has
    fn restored_len(&self) -> usize {
        let claimed = usize::from(self.len).min(64);
        (0..claimed)
            .take_while(|index| self.rows & (1 << index) != 0)
            .count()
    }

    // whether flash disagrees with what was restored: a record claiming more
    // rows, or rows held past the end. Left alone, a later grow that loses
    // power before its record would expose those stale rows again
    fn needs_repair(&self) -> bool {
        let restored = self.restored_len();
        let held_past_end = u32::try_from(restored)
            .ok()
            .and_then(|shift| self.rows.checked_shr(shift))
            .unwrap_or(0);
        usize::from(self.len) != restored || held_past_end != 0
    }
}

// what one pass over the map found for the tables of an information base
//
// items come back in the order they were written, so a table's length record
// and its rows can arrive in any order. Both are recorded while the values are
// imported and resolved once the pass is done
struct RestoredTables {
    tables: heapless::Vec<RestoredTable, MAX_TABLES>,
}

impl RestoredTables {
    const fn new() -> Self {
        Self {
            tables: heapless::Vec::new(),
        }
    }

    fn get(&self, field: u8) -> Option<&RestoredTable> {
        self.tables.iter().find(|table| table.field == field)
    }

    fn get_or_insert(&mut self, field: u8) -> Option<&mut RestoredTable> {
        if self.get(field).is_none() {
            self.tables
                .push(RestoredTable {
                    field,
                    rows: 0,
                    len: 0,
                })
                .ok()?;
        }
        self.tables.iter_mut().find(|table| table.field == field)
    }

    fn import<I: PersistentIb>(&mut self, ib: &I, field: u8, index: u16, data: &[u8]) {
        let Some(id) = I::field(field) else {
            return;
        };
        if ib.table_len(id).is_none() {
            if !ib.import_field(id, data) {
                log::warn!(
                    "[FLASH] stored {} field {field:#04x} did not parse; using default",
                    I::NAME
                );
            }
            return;
        }
        let Some(table) = self.get_or_insert(field) else {
            return;
        };
        if index == LEN_INDEX {
            if let Ok(len) = data.try_into().map(u16::from_le_bytes) {
                table.len = len;
            }
            return;
        }
        // an emptied row is how a shrunk table drops one, so it has to clear a
        // row an earlier item established
        let held = !data.is_empty() && ib.import_entry(id, usize::from(index), data);
        table.set_row(index, held);
    }

    // resolves the tables once the pass is done and leaves nothing marked
    // but what flash has to catch up on
    fn settle<I: PersistentIb>(&self, ib: &I) {
        // rows placed past the restored length were only there to keep the
        // table dense while the pass filled it in
        for field in 0..=I::MAX_KEY {
            let Some(id) = I::field(field) else {
                continue;
            };
            if ib.table_len(id).is_some() {
                let len = self.get(field).map_or(0, RestoredTable::restored_len);
                ib.truncate_table(id, len);
            }
        }

        // restore does not count as modification
        let _ = ib.take_dirty();
        for field in 0..=I::MAX_KEY {
            if let Some(id) = I::field(field) {
                let _ = ib.take_dirty_entries(id);
                let _ = ib.take_len_dirty(id);
            }
        }

        // rewriting the whole table brings flash back to what was restored
        for table in self.tables.iter().filter(|table| table.needs_repair()) {
            let Some(id) = I::field(table.field) else {
                continue;
            };
            log::debug!("[FLASH] repairing {} table {:#04x}", I::NAME, table.field);
            ib.mark_dirty(id);
        }

        // push the outgoing bounds ahead of the values just restored, before
        // any frame can be sent with them
        ib.arm_counter_bounds();
    }
}

/// An information base whose persisted fields are mirrored into the flash map.
///
/// Implemented next to the respective information base, which owns the
/// knowledge of how its fields are encoded.
pub(crate) trait PersistentIb {
    type Id: core::fmt::Debug + Copy + PartialEq + 'static;

    /// Namespaces this IB in the map key.
    const TAG: u8;
    /// Name used in log messages.
    const NAME: &'static str;
    /// Highest storage key in use.
    const MAX_KEY: u8;

    /// Resolves a storage key to its field, `None` if the key is unused.
    fn field(key: u8) -> Option<Self::Id>;

    fn dirty_bit(id: Self::Id) -> u64;

    fn take_dirty(&self) -> u64;

    fn mark_dirty(&self, id: Self::Id);

    fn import_field(&self, id: Self::Id, data: &[u8]) -> bool;

    /// Encodes a whole-field value with frame counters given their headroom.
    fn encode_field(&self, id: Self::Id, buf: &mut [u8]) -> Option<usize>;

    /// Number of rows in a table field, `None` for other fields.
    fn table_len(&self, id: Self::Id) -> Option<usize>;

    fn truncate_table(&self, id: Self::Id, len: usize);

    fn take_dirty_entries(&self, id: Self::Id) -> u64;

    /// Whether a table gained or lost rows, so its length record needs
    /// rewriting.
    fn take_len_dirty(&self, id: Self::Id) -> bool;

    /// Rows a table field can ever hold, 0 for other fields.
    fn table_capacity(&self, id: Self::Id) -> usize;

    /// Marks the fields holding outgoing frame counters.
    ///
    /// A restore leaves the live counters equal to the bound in flash, so
    /// until the next boundary a device would hand out counters flash already
    /// names, and a power loss in between would replay them. Restoring ends
    /// by storing a fresh bound ahead of them (4.3.4).
    fn arm_counter_bounds(&self);

    fn import_entry(&self, id: Self::Id, index: usize, data: &[u8]) -> bool;

    /// Encodes one table row with frame counters given their headroom.
    fn encode_entry(
        &self,
        id: Self::Id,
        index: usize,
        buf: &mut [u8],
        precision: CounterPrecision,
    ) -> Option<usize>;

    /// Marks the rows holding frame counters, so a flush rewrites the values
    /// the quiet updates left unmarked.
    fn mark_counter_rows(&self);
}

// serializes straight into the sequential-storage item buffer, so nothing is
// staged in an intermediate buffer
struct FieldValue<'a, I: PersistentIb> {
    ib: &'a I,
    id: I::Id,
    // None encodes the whole field, Some(index) one table row
    index: Option<usize>,
    precision: CounterPrecision,
}

impl<'a, I: PersistentIb> Value<'a> for FieldValue<'_, I> {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        match self.index {
            Some(index) => self.ib.encode_entry(self.id, index, buffer, self.precision),
            None => self.ib.encode_field(self.id, buffer),
        }
        .ok_or(SerializationError::BufferTooSmall)
    }

    fn deserialize_from(_buffer: &'a [u8]) -> Result<(Self, usize), SerializationError> {
        Err(SerializationError::InvalidFormat)
    }
}

// key-value flash map shared by the information bases
pub(crate) struct FlashMap<F: NorFlash> {
    map: MapStorage<u32, F, Cache>,
    scratch: [u8; SCRATCH],
}

impl<F: NorFlash> FlashMap<F> {
    fn new(flash: F, range: Range<u32>) -> Self {
        Self {
            map: MapStorage::new(flash, MapConfig::new(range), Cache::new()),
            scratch: [0; SCRATCH],
        }
    }

    // wear-leveled and crash-safe
    async fn store<'a, V: Value<'a>>(&mut self, key: u32, value: &V) -> bool {
        self.map
            .store_item(&mut self.scratch, &key, value)
            .await
            .is_ok()
    }

    /// Restores all persisted fields of both information bases in one pass
    /// over the map; missing or unparsable items keep their defaults.
    pub(crate) async fn restore<A: PersistentIb, B: PersistentIb>(&mut self, a: &A, b: &B) {
        let mut a_tables = RestoredTables::new();
        let mut b_tables = RestoredTables::new();

        // the pass hands back every item together with its value, so nothing
        // has to be looked up again. The same key can appear more than once and
        // the last one is the live value, which importing as they come leaves
        // applied
        if let Ok(mut items) = self.map.fetch_all_items(&mut self.scratch).await {
            while let Ok(Some((key, data))) = items.next::<&[u8]>(&mut self.scratch).await {
                let [tag, field, index_high, index_low] = key.to_be_bytes();
                let index = u16::from_be_bytes([index_high, index_low]);
                if tag == A::TAG {
                    a_tables.import(a, field, index, data);
                } else if tag == B::TAG {
                    b_tables.import(b, field, index, data);
                }
            }
        }

        a_tables.settle(a);
        b_tables.settle(b);
        // the incoming counters just restored are exactly what flash held, so
        // writing any of them back exactly keeps what a shutdown stored
        // instead of rounding it down
        self.flush(a, CounterPrecision::Exact).await;
        self.flush(b, CounterPrecision::Exact).await;
    }

    /// Persists everything modified since the last call; table fields write only
    /// the rows that changed.
    pub(crate) async fn flush<I: PersistentIb>(&mut self, ib: &I, precision: CounterPrecision) {
        let dirty = ib.take_dirty();

        for field in 0..=I::MAX_KEY {
            let Some(id) = I::field(field) else {
                continue;
            };
            let field_dirty = dirty & I::dirty_bit(id) != 0;

            let stored = match ib.table_len(id) {
                Some(len) => {
                    // always taken, so both dirty sets are cleared either way
                    let rows = ib.take_dirty_entries(id);
                    let len_changed = ib.take_len_dirty(id);
                    // a failed store or a repair rewrites the whole table
                    let rows = if field_dirty { u64::MAX } else { rows };
                    if rows == 0 && !len_changed && !field_dirty {
                        continue;
                    }
                    // a row updated in place leaves the length alone, so the
                    // record only needs rewriting when it actually moved. The
                    // retry path re-marks the field, which forces it again
                    let capacity = ib.table_capacity(id);
                    self.store_table(
                        ib,
                        id,
                        field,
                        len,
                        capacity,
                        rows,
                        len_changed || field_dirty,
                        precision,
                    )
                    .await
                }
                None if field_dirty => {
                    log::trace!("[FLASH] store key={field} id={id:?}");
                    let key = item_key(I::TAG, field, 0);
                    self.store(
                        key,
                        &FieldValue {
                            ib,
                            id,
                            index: None,
                            precision,
                        },
                    )
                    .await
                }
                None => continue,
            };

            if !stored {
                // retry at the next flush
                ib.mark_dirty(id);
                log::trace!("[FLASH] storing {} field {field:#04x} failed", I::NAME);
            }
        }
    }

    async fn store_table<I: PersistentIb>(
        &mut self,
        ib: &I,
        id: I::Id,
        field: u8,
        len: usize,
        capacity: usize,
        rows: u64,
        store_len: bool,
        precision: CounterPrecision,
    ) -> bool {
        let Ok(len_record) = u16::try_from(len) else {
            return false;
        };

        // rows go first and the length record last, so a power loss in between
        // leaves the old record: a grow keeps its new rows hidden, a shrink
        // has already emptied the rows it drops
        for index in 0..capacity.min(TRACKED_ENTRIES) {
            if rows & (1 << index) == 0 {
                continue;
            }
            let Ok(row) = u16::try_from(index) else {
                continue;
            };
            let key = item_key(I::TAG, field, row);

            let stored = if index < len {
                log::trace!("[FLASH] store key={field} id={id:?} index={index}");
                self.store(
                    key,
                    &FieldValue {
                        ib,
                        id,
                        index: Some(index),
                        precision,
                    },
                )
                .await
            } else {
                // a shrunk table must not leave its old rows readable: keys and
                // link keys have to go with a leave or factory reset (BDB 9.3).
                // Overwriting makes the previous item obsolete, which erasable
                // items would otherwise be needed for
                self.store(key, &EMPTY_ROW).await
            };
            if !stored {
                return false;
            }
        }

        !store_len
            || self
                .store(item_key(I::TAG, field, LEN_INDEX), &len_record)
                .await
    }
}

/// Persistence of the information bases over a NOR flash region.
///
/// Each persisted field is one map item, so a frame-counter update never
/// rewrites keys or tables.
pub struct FlashStorage<F: NorFlash> {
    // held across awaits during flush; flush waits via a yielding
    // try_lock loop so a concurrent flush cannot be starved
    map: spin::Mutex<FlashMap<F>>,
}

impl<F: NorFlash> FlashStorage<F> {
    /// Initializes the NIB and AIB backed by the given flash region.
    ///
    /// Restores all persisted fields on boot; fields never stored (or stored
    /// by an incompatible firmware) keep their defaults. Must be called
    /// instead of — not in addition to — the plain `init()` functions.
    /// Spawn a task running [`FlashStorage::run`] to persist changes.
    ///
    /// `range` must be erase-sector aligned and span at least two sectors so
    /// `sequential-storage` has a spare sector for garbage collection, and
    /// must not overlap the firmware image or partition table.
    pub async fn new(flash: F, range: Range<u32>) -> Self {
        nib::init();
        aib::init();

        let mut map = FlashMap::new(flash, range);
        map.restore(nib::get_ref(), aib::get_ref()).await;

        Self {
            map: spin::Mutex::new(map),
        }
    }
}

impl<F: NorFlash> FlashStorage<F> {
    async fn flush_with(&self, precision: CounterPrecision) {
        let mut map = loop {
            if let Some(map) = self.map.try_lock() {
                break map;
            }
            // another task is mid-flush; let it finish
            yield_now().await;
        };
        if precision == CounterPrecision::Exact {
            // marked under the lock, so a concurrent flush cannot consume the
            // marks and store the quantized values instead
            nib::get_ref().mark_counter_rows();
            aib::get_ref().mark_counter_rows();
        }
        map.flush(nib::get_ref(), precision).await;
        map.flush(aib::get_ref(), precision).await;
    }
}

impl<F: NorFlash> StorageDriver for FlashStorage<F> {
    /// Persists information-base changes as they happen; run this in its own
    /// task, or let the application runtime drive it.
    ///
    /// Wakes whenever a persisted NIB/AIB attribute changes and flushes the
    /// dirty fields. Counter headroom keeps the actual flash write rate far
    /// below the change rate.
    async fn run(&self) {
        loop {
            wait_any(nib::changed(), aib::changed()).await;
            self.flush().await;
        }
    }

    async fn flush(&self) {
        self.flush_with(CounterPrecision::Quantized).await;
    }

    /// Writes the incoming frame counters exactly, so the next boot resumes
    /// anti-replay at the last counter it accepted.
    async fn shutdown(&self) {
        self.flush_with(CounterPrecision::Exact).await;
    }
}

async fn wait_any(a: impl Future<Output = ()>, b: impl Future<Output = ()>) {
    let mut a = pin!(a);
    let mut b = pin!(b);
    poll_fn(|cx| {
        if a.as_mut().poll(cx).is_ready() || b.as_mut().poll(cx).is_ready() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use core::pin::pin;
    use core::task::Context;
    use core::task::RawWaker;
    use core::task::RawWakerVTable;
    use core::task::Waker;

    use sequential_storage::mock_flash::MockFlashBase;
    use sequential_storage::mock_flash::WriteCountCheck;
    use zigbee_types::IeeeAddress;
    use zigbee_types::StorageVec;

    use super::*;
    use crate::aps::aib::Aib;
    use crate::aps::aib::AibId;
    use crate::aps::aib::storage as aib_storage;
    use crate::nwk::nib::IncomingFrameCounterDescriptor;
    use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
    use crate::nwk::nib::Nib;
    use crate::nwk::nib::NibId;
    use crate::nwk::nib::storage as nib_storage;
    use crate::storage::round_down;

    // 4 pages of 4 KiB, 1-byte words: mirrors the esp32-c6 layout
    type Flash = MockFlashBase<4, 1, 4096>;

    fn block_on<F: Future>(fut: F) -> F::Output {
        fn noop(_: *const ()) {}
        fn clone(p: *const ()) -> RawWaker {
            RawWaker::new(p, &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = pin!(fut);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    fn fresh_ibs() -> (Nib, Aib) {
        let nib = Nib::new();
        let aib = Aib::new();
        (nib, aib)
    }

    // restoring no longer fetches single keys, but asserting what a row looks
    // like in flash still needs a raw read
    fn stored_row(map: &mut FlashMap<Flash>, key: u32) -> Option<&[u8]> {
        block_on(map.map.fetch_item::<&[u8]>(&mut map.scratch, &key))
            .ok()
            .flatten()
    }

    fn new_map() -> FlashMap<Flash> {
        FlashMap::new(
            Flash::new(WriteCountCheck::Twice, None, true),
            Flash::FULL_FLASH_RANGE,
        )
    }

    fn security_material() -> StorageVec<NetworkSecurityMaterialDescriptor, 2> {
        let mut set = StorageVec::new();
        let _ = set.push(NetworkSecurityMaterialDescriptor {
            key_seq_number: 0,
            key: zigbee_types::ByteArray([0xaa; 16]),
            network_key_type: 0x01,
        });
        set
    }

    #[test]
    fn outgoing_counter_survives_repeated_power_loss() {
        use crate::nwk::nib::storage as nib_storage;

        // cut the power at several points relative to the headroom boundary,
        // including inside the first window where nothing has been stored yet
        for cut in [1u32, 500, 1023, 1024, 1500] {
            let mut map = new_map();
            let mut highest = None;

            for _ in 0..4 {
                let (nib, _) = fresh_ibs();
                nib.update_security_material_set(|value| *value = security_material());
                block_on(map.restore(&nib, &Aib::new()));

                for _ in 0..cut {
                    let issued = nib_storage::take_outgoing_frame_counter(&nib).unwrap();
                    if let Some(previous) = highest {
                        assert!(
                            issued > previous,
                            "counter {issued} reused after {previous} (cut at {cut})"
                        );
                    }
                    highest = Some(issued);
                    block_on(map.flush(&nib, CounterPrecision::Quantized));
                }
                // power cut: anything not yet flushed is lost
            }
        }
    }

    #[test]
    fn setter_marks_dirty_getter_does_not() {
        let (nib, _) = fresh_ibs();
        let _ = nib.take_dirty();

        let _ = nib.network_address();
        assert_eq!(nib.take_dirty(), 0);

        nib.update_network_address(|value| *value = 0x1234);
        assert_eq!(nib.take_dirty(), NibId::network_address.bit());
        assert_eq!(nib.take_dirty(), 0);
    }

    #[test]
    fn ram_only_setter_does_not_mark_dirty() {
        let (nib, _) = fresh_ibs();
        let _ = nib.take_dirty();
        nib.update_sequence_number(|value| *value = 42);
        assert_eq!(nib.take_dirty(), 0);
    }

    #[test]
    fn setter_signals_change() {
        let (nib, _) = fresh_ibs();
        nib.update_network_address(|value| *value = 0x4321);
        block_on(nib::changed());
    }

    #[test]
    fn entry_export_is_compact_and_roundtrips() {
        let (nib, _) = fresh_ibs();
        let mut buf = [0u8; NibId::MAX_ENTRY_SIZE];

        nib.update_security_material_set(|value| *value = security_material());
        let len = nib
            .export_entry(NibId::security_material_set, 0, &mut buf)
            .unwrap();

        let (nib2, _) = fresh_ibs();
        assert!(nib2.import_entry(NibId::security_material_set, 0, &buf[..len]));
        let _ = nib2.take_dirty();
        let _ = nib2.take_dirty_entries(NibId::security_material_set);

        let restored = nib2.security_material_set();
        let material = restored.first().unwrap();
        assert_eq!(material.key_seq_number, 0);
        assert_eq!(material.key.0, [0xaa; 16]);
    }

    #[test]
    fn import_of_garbage_keeps_default() {
        let (nib, _) = fresh_ibs();
        assert!(!nib.import_field(NibId::network_address, &[0x01]));
        assert_eq!(nib.network_address(), 0xffff);
    }

    #[test]
    fn storage_keys_are_bijective() {
        for key in 0..=NibId::MAX_KEY {
            if let Some(id) = NibId::from_storage_key(key) {
                assert_eq!(id.storage_key(), key);
            }
        }
        for key in 0..=AibId::MAX_KEY {
            if let Some(id) = AibId::from_storage_key(key) {
                assert_eq!(id.storage_key(), key);
            }
        }
    }

    #[test]
    fn restore_from_empty_flash_keeps_defaults() {
        let (nib, aib) = fresh_ibs();
        let mut map = new_map();
        block_on(map.restore(&nib, &aib));
        assert_eq!(nib.network_address(), 0xffff);
        assert_eq!(
            *aib.trust_center_address(),
            IeeeAddress(0xffff_ffff_ffff_ffff)
        );
    }

    #[test]
    fn flush_and_restore_roundtrip() {
        let (nib, aib) = fresh_ibs();
        let mut map = new_map();

        nib.update_network_address(|value| *value = 0x1234);
        nib.update_panid(|value| *value = 0xabcd);
        nib.update_extended_panid(|value| *value = 0x1122_3344_5566_7788);
        aib.update_trust_center_address(|value| *value = IeeeAddress(0xdead_beef));
        block_on(map.flush(&nib, CounterPrecision::Quantized));
        block_on(map.flush(&aib, CounterPrecision::Quantized));

        let (nib2, aib2) = fresh_ibs();
        block_on(map.restore(&nib2, &aib2));
        assert_eq!(nib2.network_address(), 0x1234);
        assert_eq!(nib2.panid(), 0xabcd);
        assert_eq!(*nib2.extended_panid(), 0x1122_3344_5566_7788);
        assert_eq!(*aib2.trust_center_address(), IeeeAddress(0xdead_beef));
    }

    #[test]
    fn restored_outgoing_counter_is_ahead_of_any_used_value() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_outgoing_frame_counter(|value| *value = 5);
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        let restored = nib2.outgoing_frame_counter();
        assert_eq!(restored, round_up(5));
        assert!(restored > 5 + HEADROOM);
    }

    #[test]
    fn outgoing_counter_writes_flash_once_per_headroom() {
        use crate::nwk::nib::storage as nib_storage;

        let writes = |ticks: u32| {
            let (nib, _) = fresh_ibs();
            let flash = Flash::new(WriteCountCheck::Twice, None, true);
            let baseline = flash.stats_snapshot();
            let mut map = FlashMap::new(flash, Flash::FULL_FLASH_RANGE);
            block_on(map.restore(&nib, &Aib::new()));

            for _ in 0..ticks {
                let _ = nib_storage::take_outgoing_frame_counter(&nib).unwrap();
                block_on(map.flush(&nib, CounterPrecision::Quantized));
            }

            let FlashMap { map, .. } = map;
            let (flash, _) = map.destroy();
            baseline.compare_to(flash.stats_snapshot()).writes
        };

        // the stored value is a bound, so ticks inside it cost nothing; only
        // the boundary crossings and the one write restore makes are paid for
        let boundary = writes(4 * HEADROOM);
        assert!(
            boundary <= 8 * (4 + 1),
            "{boundary} writes for {} ticks",
            4 * HEADROOM
        );
        assert!(writes(HEADROOM - 1) < boundary);
    }

    #[test]
    fn dropped_rows_are_overwritten_in_flash() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_security_material_set(|value| *value = security_material());
        block_on(map.flush(&nib, CounterPrecision::Quantized));
        let key = item_key(
            <Nib as PersistentIb>::TAG,
            NibId::security_material_set as u8,
            0,
        );
        assert!(stored_row(&mut map, key).is_some_and(|row| !row.is_empty()));

        // a leave or factory reset must not leave the network key readable
        nib.update_security_material_set(|set| set.clear());
        block_on(map.flush(&nib, CounterPrecision::Quantized));
        assert_eq!(stored_row(&mut map, key), Some(&[][..]));
    }

    #[test]
    fn touching_one_entry_writes_only_that_entry() {
        // both runs seed and flush the same table, so the difference in bytes
        // written comes only from the second flush
        let writes = |whole_table: bool| {
            let (nib, _) = fresh_ibs();
            let flash = Flash::new(WriteCountCheck::Twice, None, true);
            let baseline = flash.stats_snapshot();
            let mut map = FlashMap::new(flash, Flash::FULL_FLASH_RANGE);

            nib.update_group_idtable(|table| {
                for group in 0..4 {
                    let _ = table.push(group);
                }
            });
            block_on(map.flush(&nib, CounterPrecision::Quantized));

            if whole_table {
                nib.update_group_idtable(|table| table[3] = 42);
            } else {
                nib.group_idtable_mut().update(3, |entry| *entry = 42);
            }
            block_on(map.flush(&nib, CounterPrecision::Quantized));

            let FlashMap { map, .. } = map;
            let (flash, _) = map.destroy();
            baseline.compare_to(flash.stats_snapshot()).bytes_written
        };

        assert!(writes(false) < writes(true));
    }

    #[test]
    fn recording_one_incoming_counter_writes_one_row() {
        // both runs seed and flush eight senders, so the difference in bytes
        // written comes only from the second flush
        let writes = |whole_table: bool| {
            let (nib, _) = fresh_ibs();
            let flash = Flash::new(WriteCountCheck::Twice, None, true);
            let baseline = flash.stats_snapshot();
            let mut map = FlashMap::new(flash, Flash::FULL_FLASH_RANGE);

            nib.update_incoming_frame_counters(|counters| {
                for sender in 0..8 {
                    let _ = counters.push(IncomingFrameCounterDescriptor {
                        key_seq_number: 0,
                        sender_address: IeeeAddress(sender),
                        incoming_frame_counter: 0,
                    });
                }
            });
            block_on(map.flush(&nib, CounterPrecision::Quantized));

            if whole_table {
                nib.update_incoming_frame_counters(|counters| {
                    counters[3].incoming_frame_counter = 9;
                });
            } else {
                nib.incoming_frame_counters_mut()
                    .update(3, |entry| entry.incoming_frame_counter = 9);
            }
            block_on(map.flush(&nib, CounterPrecision::Quantized));

            let FlashMap { map, .. } = map;
            let (flash, _) = map.destroy();
            baseline.compare_to(flash.stats_snapshot()).bytes_written
        };

        // one sender advancing must not rewrite the other seven rows
        assert!(writes(false) < writes(true));
    }

    #[test]
    fn shutdown_flush_leaves_no_replay_window() {
        // an unclean reset gives up the window the stored value is rounded
        // down to; a shutdown flush must give up nothing
        let restored = |precision| {
            let (nib, _) = fresh_ibs();
            let mut map = new_map();
            let sender = IeeeAddress(0x1122_3344);

            for counter in 1..=100 {
                assert!(nib_storage::record_incoming_frame_counter(
                    &nib, 0, sender, counter
                ));
            }
            if precision == CounterPrecision::Exact {
                nib.mark_counter_rows();
            }
            block_on(map.flush(&nib, precision));

            let (nib2, _) = fresh_ibs();
            block_on(map.restore(&nib2, &Aib::new()));
            nib2.incoming_frame_counters()
                .first()
                .unwrap()
                .incoming_frame_counter
        };

        assert_eq!(restored(CounterPrecision::Exact), 100);
        assert_eq!(restored(CounterPrecision::Quantized), round_down(100));
    }

    #[test]
    fn whole_table_update_marks_only_the_rows_it_held() {
        let (nib, _) = fresh_ibs();
        nib.update_incoming_frame_counters(|counters| {
            for sender in 0..2 {
                let _ = counters.push(IncomingFrameCounterDescriptor {
                    key_seq_number: 0,
                    sender_address: IeeeAddress(sender),
                    incoming_frame_counter: 0,
                });
            }
        });
        let id = NibId::incoming_frame_counters;
        let _ = nib.take_dirty_entries(id);
        let _ = nib.take_len_dirty(id);

        // rows past the length were empty before and stay empty, and an update
        // in place leaves the length record alone
        nib.update_incoming_frame_counters(|counters| counters[1].incoming_frame_counter = 7);
        assert_eq!(nib.take_dirty_entries(id), 0b11);
        assert!(!nib.take_len_dirty(id));
        assert_eq!(nib.take_dirty() & id.bit(), 0);

        // a shrink still marks the row it dropped
        nib.update_incoming_frame_counters(|counters| {
            counters.pop();
        });
        assert_eq!(nib.take_dirty_entries(id), 0b11);
        assert!(nib.take_len_dirty(id));
    }

    #[test]
    fn restore_keeps_the_exact_counters_a_shutdown_stored() {
        let (_, aib) = fresh_ibs();
        let mut map = new_map();
        let peer = IeeeAddress(0xaabb);

        assert!(aib_storage::record_incoming_frame_counter(
            &aib, peer, 100, [0xbb; 16]
        ));
        aib.mark_counter_rows();
        block_on(map.flush(&aib, CounterPrecision::Exact));

        // restoring rewrites the row to push its outgoing bound ahead; that
        // must not round the incoming counter down on the way
        for _ in 0..2 {
            let (nib2, aib2) = fresh_ibs();
            block_on(map.restore(&nib2, &aib2));
            let pairs = aib2.device_key_pair_set();
            assert_eq!(pairs.first().unwrap().incoming_frame_counter, 100);
        }
    }

    #[test]
    fn rows_are_restored_when_they_arrive_out_of_order() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_group_idtable(|table| {
            for group in [10u16, 11, 12] {
                let _ = table.push(group);
            }
        });

        // only dirty rows are rewritten, so the newest copy of a row can sit
        // ahead of the rows below it. Writing them in reverse makes the pass
        // meet row 2 while the table is still empty
        let field = NibId::group_idtable as u8;
        let tag = <Nib as PersistentIb>::TAG;
        let mut row = [0u8; 16];
        for index in [2usize, 1, 0] {
            let len = nib
                .export_entry(NibId::group_idtable, index, &mut row)
                .unwrap();
            assert!(block_on(
                map.store(item_key(tag, field, index as u16), &&row[..len])
            ));
        }
        assert!(block_on(map.store(item_key(tag, field, LEN_INDEX), &3u16)));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        assert_eq!(nib2.group_idtable().as_slice(), &[10, 11, 12]);
    }

    #[test]
    fn a_table_with_a_missing_row_is_restored_up_to_the_hole() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_group_idtable(|table| {
            for group in 0..4 {
                let _ = table.push(group);
            }
        });
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        // a power loss between the length record and the rows leaves a table
        // longer than what was written
        let field = NibId::group_idtable as u8;
        assert!(block_on(map.store(
            item_key(<Nib as PersistentIb>::TAG, field, LEN_INDEX),
            &6u16
        )));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        assert_eq!(nib2.group_idtable().as_slice(), &[0, 1, 2, 3]);
    }

    #[test]
    fn an_interrupted_shrink_is_repaired_on_restore() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_group_idtable(|table| {
            for group in 0..4 {
                let _ = table.push(group);
            }
        });
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        // a shrink to two rows loses power after emptying row 2: the record
        // still claims four and row 3 still holds its old value
        let tag = <Nib as PersistentIb>::TAG;
        let field = NibId::group_idtable as u8;
        assert!(block_on(map.store(item_key(tag, field, 2), &EMPTY_ROW)));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        assert_eq!(nib2.group_idtable().as_slice(), &[0, 1]);
        assert_eq!(stored_row(&mut map, item_key(tag, field, 3)), Some(&[][..]));
        assert_eq!(
            stored_row(&mut map, item_key(tag, field, LEN_INDEX)),
            Some(&2u16.to_le_bytes()[..])
        );

        // a later grow losing power before its record must not bring the old
        // row 3 back along with the new row 2
        let mut row = [0u8; 16];
        nib2.update_group_idtable(|table| {
            let _ = table.push(9);
        });
        let len = nib2
            .export_entry(NibId::group_idtable, 2, &mut row)
            .unwrap();
        assert!(block_on(map.store(item_key(tag, field, 2), &&row[..len])));

        let (nib3, _) = fresh_ibs();
        block_on(map.restore(&nib3, &Aib::new()));
        assert_eq!(nib3.group_idtable().as_slice(), &[0, 1]);
    }

    #[test]
    fn shrinking_a_table_is_restored_at_the_new_length() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_group_idtable(|table| {
            for group in 0..4 {
                let _ = table.push(group);
            }
        });
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        nib.group_idtable_mut().remove(1);
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        assert_eq!(nib2.group_idtable().as_slice(), &[0, 2, 3]);
    }

    #[test]
    fn clearing_a_table_is_restored_as_empty() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_group_idtable(|table| {
            for group in 0..4 {
                let _ = table.push(group);
            }
        });
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        nib.group_idtable_mut().clear();
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        assert!(nib2.group_idtable().is_empty());
    }

    #[test]
    fn regrowing_after_a_shrink_does_not_resurrect_old_rows() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();

        nib.update_group_idtable(|table| {
            for group in 0..4 {
                let _ = table.push(group);
            }
        });
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        nib.group_idtable_mut().clear();
        block_on(map.flush(&nib, CounterPrecision::Quantized));
        let mut table = nib.group_idtable_mut();
        let _ = table.push(77);
        let _ = table.push(88);
        drop(table);
        block_on(map.flush(&nib, CounterPrecision::Quantized));

        let (nib2, _) = fresh_ibs();
        block_on(map.restore(&nib2, &Aib::new()));
        assert_eq!(nib2.group_idtable().as_slice(), &[77, 88]);
    }

    #[test]
    fn failed_store_rearms_dirty_bit() {
        let (nib, _) = fresh_ibs();
        let mut flash = Flash::new(WriteCountCheck::Twice, None, true);
        flash.bytes_until_shutoff = Some(0);
        let mut map = FlashMap::new(flash, Flash::FULL_FLASH_RANGE);

        nib.update_network_address(|value| *value = 0x1234);
        block_on(map.flush(&nib, CounterPrecision::Quantized));
        assert_eq!(nib.take_dirty(), NibId::network_address.bit());
    }
}
