//! flash persistence for the zigbee information bases
//!
//! the zigbee crate keeps the NIB/AIB in a RAM-backed `embedded_storage`
//! mirror. this module snapshots that mirror to NOR flash and restores it on
//! boot so keys, frame counters and the neighbor table survive a reset.
//!
//! the application owns this entirely: it decides the flash region and the
//! flush cadence. the library never touches flash, so it can never clobber a
//! region the firmware uses for something else.

use core::ops::Range;

use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::cache::NoCache;
use sequential_storage::map::MapConfig;
use sequential_storage::map::MapStorage;

/// flash region reserved for zigbee persistence.
///
/// MUST be adjusted to your partition table: it must not overlap the
/// bootloader, the firmware image, or the esp-idf partition table. it must be
/// erase-sector aligned (4 KiB on esp32-c6) and span at least two sectors so
/// sequential-storage has a spare sector for garbage collection.
pub const ZIGBEE_FLASH_RANGE: Range<u32> = 0x3f_0000..0x3f_4000;

/// distinguishes the information bases inside one shared flash map.
///
/// using one key per IB is what stops the NIB and AIB from overwriting each
/// other — they live in the same region but under different keys.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Region {
    Nib = 0,
    Aib = 1,
}

/// scratch buffer for sequential-storage. must be >= the largest IB blob.
/// NIB is the larger of the two; bump this if the IB grows.
const SCRATCH: usize = 2048;

// a growing neighbor table would otherwise only surface as a runtime save error
const _: () = assert!(SCRATCH >= zigbee::nwk::nib::NibId::BUFFER_SIZE);
const _: () = assert!(SCRATCH >= zigbee::aps::aib::AibId::BUFFER_SIZE);

/// key-value persistence over a NOR flash region.
pub struct Persist<F: NorFlash> {
    map: MapStorage<u8, F, NoCache>,
    scratch: [u8; SCRATCH],
}

impl<F: NorFlash> Persist<F> {
    /// `range` must be flash-erase-sector aligned and span at least two
    /// sectors (sequential-storage needs a spare sector for garbage
    /// collection). it must not overlap the firmware or partition table.
    pub fn new(flash: F, range: Range<u32>) -> Self {
        let map = MapStorage::new(flash, MapConfig::new(range), NoCache::new());
        Self { map, scratch: [0u8; SCRATCH] }
    }

    /// loads a persisted IB blob, returning `None` on first boot.
    pub async fn load(&mut self, region: Region) -> Option<&[u8]> {
        self.map.fetch_item::<&[u8]>(&mut self.scratch, &(region as u8)).await.ok().flatten()
    }

    /// persists an IB blob. wear-leveled and crash-safe by sequential-storage;
    /// returns `FullStorage` if the region is too small even after GC.
    pub async fn save(
        &mut self,
        region: Region,
        blob: &[u8],
    ) -> Result<(), sequential_storage::Error<F::Error>> {
        self.map.store_item(&mut self.scratch, &(region as u8), &blob).await
    }
}
