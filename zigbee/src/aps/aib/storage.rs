//! Flash persistence of the AIB (see [`crate::storage`]).

use byte::BytesExt;
use embedded_storage_async::nor_flash::NorFlash;

use super::Aib;
use super::AibId;
use crate::storage::FlashMap;
use crate::storage::Shadow;
use crate::storage::round_down;
use crate::storage::round_up;

// upper byte of the flash map key namespaces the AIB
const TAG: u16 = 0x0100;

/// Restores all persisted AIB fields; missing or unparsable items keep
/// their defaults.
pub(crate) async fn restore<F: NorFlash>(map: &mut FlashMap<F>, aib: &Aib) {
    for id in AibId::VARIANTS {
        let Some(key) = id.storage_key() else {
            continue;
        };
        let key = TAG | u16::from(key);
        if let Some(data) = map.fetch(key).await
            && !aib.import_field(*id, data)
        {
            log::warn!("stored AIB field {key:#06x} did not parse; using default");
        }
    }
    // restore does not count as modification
    let _ = aib.take_dirty();
}

/// Persists all AIB fields modified since the last call.
pub(crate) async fn flush<F: NorFlash>(map: &mut FlashMap<F>, shadow: &mut Shadow, aib: &Aib) {
    let dirty = aib.take_dirty();
    if dirty == 0 {
        return;
    }

    for id in AibId::VARIANTS {
        if dirty & (1 << (*id as u64)) == 0 {
            continue;
        }
        let Some(key) = id.storage_key() else {
            continue;
        };

        let len = if *id == AibId::device_key_pair_set {
            // normalize counters so the stored image only changes when a
            // counter crosses its headroom boundary
            let mut set = ::core::clone::Clone::clone(&*aib.device_key_pair_set());
            for pair in set.iter_mut() {
                pair.outgoing_frame_counter = round_up(pair.outgoing_frame_counter);
                pair.incoming_frame_counter = round_down(pair.incoming_frame_counter);
            }
            let mut offset = 0;
            let Ok(()) = map.data.write_with(&mut offset, set, byte::LE) else {
                continue;
            };
            if shadow.1 == offset && shadow.0[..offset] == map.data[..offset] {
                continue;
            }
            offset
        } else {
            let Some(len) = aib.export_field(*id, &mut map.data) else {
                continue;
            };
            len
        };

        let key = TAG | u16::from(key);
        if map.store(key, len).await {
            if *id == AibId::device_key_pair_set {
                shadow.0[..len].copy_from_slice(&map.data[..len]);
                shadow.1 = len;
            }
        } else {
            // retry at the next flush
            aib.mark_dirty(*id);
            log::debug!("storing AIB field {key:#06x} failed");
        }
    }
}
