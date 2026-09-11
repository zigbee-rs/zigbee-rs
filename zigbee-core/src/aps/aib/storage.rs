//! AIB persistence: how its fields reach flash, and the frame-counter policy
//! deciding when they need to (see [`crate::storage`]).

#[cfg(feature = "storage")]
use byte::BytesExt;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;

use super::Aib;
use super::AibId;
use super::DeviceKeyPairDescriptor;
use super::KeyAttribute;
use super::LinkKeyType;
#[cfg(feature = "storage")]
use crate::storage::PersistentIb;
use crate::storage::round_down;
use crate::storage::round_up;

/// Advances the outgoing APS frame counter of the key pair shared with
/// `device`.
pub(crate) fn advance_outgoing_frame_counter(aib: &Aib, device: IeeeAddress) {
    let mut pairs = aib.device_key_pair_set_mut();
    let Some(index) = pairs.position(|pair| pair.device_address == device) else {
        return;
    };
    let previous = pairs
        .get(index)
        .map_or(0, |pair| pair.outgoing_frame_counter);
    let next = previous.wrapping_add(1);
    // as for the NWK counter, only a crossing moves the stored bound
    if round_up(next) == round_up(previous) {
        pairs.update_quiet(index, |pair| pair.outgoing_frame_counter = next);
    } else {
        pairs.update(index, |pair| pair.outgoing_frame_counter = next);
    }
}

/// Records `counter` as the most recently accepted incoming APS counter for
/// `device`, adding a key pair for a first-time device.
///
/// Returns `false` when the table is full, so the caller can reject the frame
/// instead of accepting one whose counter cannot be tracked.
pub(crate) fn record_incoming_frame_counter(
    aib: &Aib,
    device: IeeeAddress,
    counter: u32,
    default_link_key: [u8; 16],
) -> bool {
    let mut pairs = aib.device_key_pair_set_mut();
    let Some(index) = pairs.position(|pair| pair.device_address == device) else {
        let stored = pairs
            .push(DeviceKeyPairDescriptor {
                device_address: device,
                key_attributes: KeyAttribute::VerifiedKey,
                link_key: ByteArray(default_link_key),
                outgoing_frame_counter: 0,
                incoming_frame_counter: counter,
                link_key_type: LinkKeyType::GlobalLinkKey,
            })
            .is_ok();
        if !stored {
            log::warn!("[APS] key pair table full, rejecting frame from {device:?}");
        }
        return stored;
    };
    let previous = pairs
        .get(index)
        .map_or(0, |pair| pair.incoming_frame_counter);
    if round_down(counter) == round_down(previous) {
        pairs.update_quiet(index, |pair| pair.incoming_frame_counter = counter);
    } else {
        pairs.update(index, |pair| pair.incoming_frame_counter = counter);
    }
    true
}

#[cfg(feature = "storage")]
impl PersistentIb for Aib {
    type Id = AibId;

    const TAG: u8 = 0x01;
    const NAME: &'static str = "AIB";
    const MAX_KEY: u8 = AibId::MAX_KEY;

    fn field(key: u8) -> Option<AibId> {
        AibId::from_storage_key(key)
    }

    fn dirty_bit(id: AibId) -> u64 {
        id.bit()
    }

    fn take_dirty(&self) -> u64 {
        Self::take_dirty(self)
    }

    fn mark_dirty(&self, id: AibId) {
        Self::mark_dirty(self, id);
    }

    fn import_field(&self, id: AibId, data: &[u8]) -> bool {
        Self::import_field(self, id, data)
    }

    fn encode_field(&self, id: AibId, buf: &mut [u8]) -> Option<usize> {
        self.export_field(id, buf)
    }

    fn table_len(&self, id: AibId) -> Option<usize> {
        Self::table_len(self, id)
    }

    fn truncate_table(&self, id: AibId, len: usize) {
        Self::truncate_table(self, id, len);
    }

    fn take_dirty_entries(&self, id: AibId) -> u64 {
        Self::take_dirty_entries(self, id)
    }

    fn take_len_dirty(&self, id: AibId) -> bool {
        Self::take_len_dirty(self, id)
    }

    fn table_capacity(&self, id: AibId) -> usize {
        Self::table_capacity(self, id)
    }

    fn arm_counter_bounds(&self) {
        Self::mark_dirty(self, AibId::device_key_pair_set);
    }

    fn import_entry(&self, id: AibId, index: usize, data: &[u8]) -> bool {
        Self::import_entry(self, id, index, data)
    }

    fn encode_entry(&self, id: AibId, index: usize, buf: &mut [u8]) -> Option<usize> {
        // the pair's counters are quantized like the NWK ones
        if id == AibId::device_key_pair_set {
            let table = self.device_key_pair_set();
            let mut pair = Clone::clone(table.get(index)?);
            drop(table);
            pair.outgoing_frame_counter = round_up(pair.outgoing_frame_counter);
            pair.incoming_frame_counter = round_down(pair.incoming_frame_counter);
            let mut offset = 0;
            buf.write_with(&mut offset, pair, byte::LE).ok()?;
            return Some(offset);
        }
        Self::export_entry(self, id, index, buf)
    }
}
