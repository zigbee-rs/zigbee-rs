use core::future::poll_fn;
use core::ops::Range;
use core::pin::pin;
use core::task::Poll;

use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::cache::NoCache;
use sequential_storage::map::MapConfig;
use sequential_storage::map::MapStorage;

use super::StorageDriver;
use crate::aps::aib;
use crate::nwk::nib;

// outgoing frame counters are stored this far ahead of the live value
const HEADROOM: u32 = 1024;
// incoming frame counters are stored rounded down to this granularity
const WINDOW: u32 = 1024;

pub(crate) const DATA: usize = {
    let a = nib::NibId::MAX_FIELD_SIZE;
    let b = aib::AibId::MAX_FIELD_SIZE;
    if a > b { a } else { b }
};
// sequential-storage work buffer: encoded field + item overhead
const SCRATCH: usize = DATA + 64;
// every map item must fit into one flash sector (4 KiB on esp32)
const _: () = assert!(SCRATCH <= 4096);

// last-stored normalized image of a counter-bearing field, used to skip
// writes while counters stay within their stored headroom
pub(crate) type Shadow = ([u8; DATA], usize);

// next counter value a rebooted device may use; two boundaries ahead so
// the stored bound is refreshed a full HEADROOM before it could be reached
pub(crate) const fn round_up(counter: u32) -> u32 {
    (counter / HEADROOM)
        .saturating_add(2)
        .saturating_mul(HEADROOM)
}

pub(crate) const fn round_down(counter: u32) -> u32 {
    (counter / WINDOW) * WINDOW
}

// key-value flash map shared by the information bases; `data` is the
// staging buffer, encode a field into it then persist the prefix via store
pub(crate) struct FlashMap<F: NorFlash> {
    map: MapStorage<u16, F, NoCache>,
    scratch: [u8; SCRATCH],
    pub(crate) data: [u8; DATA],
}

impl<F: NorFlash> FlashMap<F> {
    fn new(flash: F, range: Range<u32>) -> Self {
        Self {
            map: MapStorage::new(flash, MapConfig::new(range), NoCache::new()),
            scratch: [0; SCRATCH],
            data: [0; DATA],
        }
    }

    pub(crate) async fn fetch(&mut self, key: u16) -> Option<&[u8]> {
        self.map
            .fetch_item::<&[u8]>(&mut self.scratch, &key)
            .await
            .ok()
            .flatten()
    }

    // persists data[..len] under key; wear-leveled and crash-safe
    pub(crate) async fn store(&mut self, key: u16, len: usize) -> bool {
        let value: &[u8] = &self.data[..len];
        self.map
            .store_item(&mut self.scratch, &key, &value)
            .await
            .is_ok()
    }
}

/// Persistence of the information bases over a NOR flash region.
///
/// Each persisted field is one map item, so a frame-counter update never
/// rewrites keys or tables.
pub struct FlashStorage<F: NorFlash> {
    // held across awaits during flush; flush waits via a yielding
    // try_lock loop so a concurrent flush cannot be starved
    inner: spin::Mutex<Inner<F>>,
}

struct Inner<F: NorFlash> {
    map: FlashMap<F>,
    nib_shadow: Shadow,
    aib_shadow: Shadow,
}

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
pub async fn init_with_flash<F: NorFlash>(flash: F, range: Range<u32>) -> FlashStorage<F> {
    nib::init();
    aib::init();

    let mut map = FlashMap::new(flash, range);
    nib::storage::restore(&mut map, nib::get_ref()).await;
    aib::storage::restore(&mut map, aib::get_ref()).await;

    FlashStorage {
        inner: spin::Mutex::new(Inner {
            map,
            nib_shadow: ([0; DATA], 0),
            aib_shadow: ([0; DATA], 0),
        }),
    }
}

impl<F: NorFlash> StorageDriver for FlashStorage<F> {
    async fn flush(&self) {
        let mut inner = loop {
            if let Some(inner) = self.inner.try_lock() {
                break inner;
            }
            // another task is mid-flush; let it finish
            yield_now().await;
        };
        let Inner {
            map,
            nib_shadow,
            aib_shadow,
        } = &mut *inner;
        nib::storage::flush(map, nib_shadow, nib::get_ref()).await;
        aib::storage::flush(map, aib_shadow, aib::get_ref()).await;
    }
}

impl<F: NorFlash> FlashStorage<F> {
    /// Persists information-base changes as they happen; run this in its
    /// own task.
    ///
    /// Wakes whenever a persisted NIB/AIB attribute changes and flushes
    /// the dirty fields. Counter headroom keeps the actual flash write
    /// rate far below the change rate.
    pub async fn run(&self) -> ! {
        loop {
            wait_any(nib::changed(), aib::changed()).await;
            self.flush().await;
        }
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

async fn yield_now() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
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
    use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
    use crate::nwk::nib::Nib;
    use crate::nwk::nib::NibId;

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

    fn new_map() -> FlashMap<Flash> {
        FlashMap::new(
            Flash::new(WriteCountCheck::Twice, None, true),
            Flash::FULL_FLASH_RANGE,
        )
    }

    fn security_material(counter: u32) -> StorageVec<NetworkSecurityMaterialDescriptor, 2> {
        let mut set = StorageVec::new();
        let _ = set.push(NetworkSecurityMaterialDescriptor {
            key_seq_number: 0,
            outgoing_frame_counter: counter,
            incoming_frame_counter_set: StorageVec::new(),
            key: zigbee_types::ByteArray([0xaa; 16]),
            network_key_type: 0x01,
        });
        set
    }

    #[test]
    fn setter_marks_dirty_getter_does_not() {
        let (nib, _) = fresh_ibs();
        let _ = nib.take_dirty();

        let _ = nib.network_address();
        assert_eq!(nib.take_dirty(), 0);

        nib.update_network_address(|value| *value = 0x1234);
        assert_eq!(nib.take_dirty(), 1 << (NibId::network_address as u64));
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
    fn export_is_compact_and_roundtrips() {
        let (nib, _) = fresh_ibs();
        let mut buf = [0u8; NibId::MAX_FIELD_SIZE];

        let len = nib.export_field(NibId::neighbor_table, &mut buf).unwrap();
        assert_eq!(len, 2);

        nib.update_security_material_set(|value| *value = security_material(7));
        let len = nib
            .export_field(NibId::security_material_set, &mut buf)
            .unwrap();

        let (nib2, _) = fresh_ibs();
        let _ = nib2.take_dirty();
        assert!(nib2.import_field(NibId::security_material_set, &buf[..len]));
        let restored = nib2.security_material_set();
        assert_eq!(restored.first().unwrap().outgoing_frame_counter, 7);
        assert_eq!(restored.first().unwrap().key.0, [0xaa; 16]);
        assert_eq!(nib2.take_dirty(), 0);
    }

    #[test]
    fn import_of_garbage_keeps_default() {
        let (nib, _) = fresh_ibs();
        assert!(!nib.import_field(NibId::network_address, &[0x01]));
        assert_eq!(*nib.network_address(), 0xffff);
    }

    #[test]
    fn storage_keys_are_bijective() {
        for id in NibId::VARIANTS {
            if let Some(key) = id.storage_key() {
                assert_eq!(NibId::from_storage_key(key), Some(*id));
            }
        }
        for id in AibId::VARIANTS {
            if let Some(key) = id.storage_key() {
                assert_eq!(AibId::from_storage_key(key), Some(*id));
            }
        }
    }

    #[test]
    fn restore_from_empty_flash_keeps_defaults() {
        let (nib, aib) = fresh_ibs();
        let mut map = new_map();
        block_on(nib::storage::restore(&mut map, &nib));
        block_on(aib::storage::restore(&mut map, &aib));
        assert_eq!(*nib.network_address(), 0xffff);
        assert_eq!(
            *aib.trust_center_address(),
            IeeeAddress(0xffff_ffff_ffff_ffff)
        );
    }

    #[test]
    fn flush_and_restore_roundtrip() {
        let (nib, aib) = fresh_ibs();
        let mut map = new_map();
        let mut nib_shadow: Shadow = ([0; DATA], 0);
        let mut aib_shadow: Shadow = ([0; DATA], 0);

        nib.update_network_address(|value| *value = 0x1234);
        nib.update_panid(|value| *value = 0xabcd);
        nib.update_extended_panid(|value| *value = 0x1122_3344_5566_7788);
        aib.update_trust_center_address(|value| *value = IeeeAddress(0xdead_beef));
        block_on(nib::storage::flush(&mut map, &mut nib_shadow, &nib));
        block_on(aib::storage::flush(&mut map, &mut aib_shadow, &aib));

        let (nib2, aib2) = fresh_ibs();
        block_on(nib::storage::restore(&mut map, &nib2));
        block_on(aib::storage::restore(&mut map, &aib2));
        assert_eq!(*nib2.network_address(), 0x1234);
        assert_eq!(*nib2.panid(), 0xabcd);
        assert_eq!(*nib2.extended_panid(), 0x1122_3344_5566_7788);
        assert_eq!(*aib2.trust_center_address(), IeeeAddress(0xdead_beef));
    }

    #[test]
    fn restored_outgoing_counter_is_ahead_of_any_used_value() {
        let (nib, _) = fresh_ibs();
        let mut map = new_map();
        let mut shadow: Shadow = ([0; DATA], 0);

        nib.update_security_material_set(|value| *value = security_material(5));
        block_on(nib::storage::flush(&mut map, &mut shadow, &nib));

        let (nib2, _) = fresh_ibs();
        block_on(nib::storage::restore(&mut map, &nib2));
        let restored = nib2
            .security_material_set()
            .first()
            .unwrap()
            .outgoing_frame_counter;
        assert_eq!(restored, round_up(5));
        assert!(restored > 5 + HEADROOM);
    }

    #[test]
    fn counter_ticks_within_headroom_do_not_write_flash() {
        let flush_writes = |ticks: u32| {
            let (nib, _) = fresh_ibs();
            let flash = Flash::new(WriteCountCheck::Twice, None, true);
            let baseline = flash.stats_snapshot();
            let mut map = FlashMap::new(flash, Flash::FULL_FLASH_RANGE);
            let mut shadow: Shadow = ([0; DATA], 0);

            nib.update_security_material_set(|value| *value = security_material(0));
            block_on(nib::storage::flush(&mut map, &mut shadow, &nib));
            for counter in 1..=ticks {
                nib.update_security_material_set(|value| *value = security_material(counter));
                block_on(nib::storage::flush(&mut map, &mut shadow, &nib));
            }

            let FlashMap { map, .. } = map;
            let (flash, _) = map.destroy();
            baseline.compare_to(flash.stats_snapshot()).bytes_written
        };

        assert_eq!(flush_writes(0), flush_writes(HEADROOM - 1));
        assert!(flush_writes(HEADROOM) > flush_writes(HEADROOM - 1));
    }

    #[test]
    fn failed_store_rearms_dirty_bit() {
        let (nib, _) = fresh_ibs();
        let mut flash = Flash::new(WriteCountCheck::Twice, None, true);
        flash.bytes_until_shutoff = Some(0);
        let mut map = FlashMap::new(flash, Flash::FULL_FLASH_RANGE);
        let mut shadow: Shadow = ([0; DATA], 0);

        nib.update_network_address(|value| *value = 0x1234);
        block_on(nib::storage::flush(&mut map, &mut shadow, &nib));
        assert_eq!(nib.take_dirty(), 1 << (NibId::network_address as u64));
    }
}
