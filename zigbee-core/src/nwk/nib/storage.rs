//! NIB persistence: how its fields reach flash, and the frame-counter policy
//! deciding when they need to (see [`crate::storage`]).

#[cfg(feature = "storage")]
use byte::BytesExt;
use zigbee_types::IeeeAddress;

use super::IncomingFrameCounterDescriptor;
use super::Nib;
use super::NibId;
#[cfg(feature = "storage")]
use crate::storage::PersistentIb;
use crate::storage::round_down;
use crate::storage::round_up;

/// Takes the outgoing NWK frame counter and advances it (4.3.1.1).
///
/// One counter is shared by every security material set (4.3.4).
pub(crate) fn take_outgoing_frame_counter(nib: &Nib) -> u32 {
    let mut counter = 0;
    nib.update_outgoing_frame_counter(|value| {
        counter = *value;
        *value = value.wrapping_add(1);
    });
    // the stored value is a bound, so only a crossing changes it; the encrypt
    // path is its only writer
    if round_up(counter.wrapping_add(1)) == round_up(counter) {
        nib.unmark_dirty(NibId::outgoing_frame_counter);
    }
    counter
}

/// Records `counter` as the most recently accepted incoming counter for
/// `(key_seq_number, sender_address)`, adding a row for a first-time sender.
///
/// Returns `false` when the table is full.
pub(crate) fn record_incoming_frame_counter(
    nib: &Nib,
    key_seq_number: u8,
    sender_address: IeeeAddress,
    counter: u32,
) -> bool {
    let mut counters = nib.incoming_frame_counters_mut();
    let Some(index) = counters
        .position(|i| i.key_seq_number == key_seq_number && i.sender_address == sender_address)
    else {
        return counters
            .push(IncomingFrameCounterDescriptor {
                key_seq_number,
                sender_address,
                incoming_frame_counter: counter,
            })
            .is_ok();
    };

    let previous = counters
        .get(index)
        .map_or(0, |entry| entry.incoming_frame_counter);
    // the row stores the counter rounded down, so only a crossing needs
    // persisting; in between the live value moves silently
    if round_down(counter) == round_down(previous) {
        counters.update_quiet(index, |entry| entry.incoming_frame_counter = counter);
    } else {
        counters.update(index, |entry| entry.incoming_frame_counter = counter);
    }
    true
}

#[cfg(feature = "storage")]
impl PersistentIb for Nib {
    type Id = NibId;

    const TAG: u8 = 0x00;
    const NAME: &'static str = "NIB";
    const MAX_KEY: u8 = NibId::MAX_KEY;

    fn field(key: u8) -> Option<NibId> {
        NibId::from_storage_key(key)
    }

    fn dirty_bit(id: NibId) -> u64 {
        id.bit()
    }

    fn take_dirty(&self) -> u64 {
        Self::take_dirty(self)
    }

    fn mark_dirty(&self, id: NibId) {
        Self::mark_dirty(self, id);
    }

    fn import_field(&self, id: NibId, data: &[u8]) -> bool {
        Self::import_field(self, id, data)
    }

    fn encode_field(&self, id: NibId, buf: &mut [u8]) -> Option<usize> {
        // store a bound the live counter cannot have reached yet, so a reset
        // never hands out a counter that was already transmitted (4.3.4). It
        // only moves once per HEADROOM, which is what keeps the write rate
        // far below the frame rate
        if id == NibId::outgoing_frame_counter {
            let mut offset = 0;
            let bound = round_up(self.outgoing_frame_counter());
            buf.write_with(&mut offset, bound, byte::LE).ok()?;
            return Some(offset);
        }
        self.export_field(id, buf)
    }

    fn table_len(&self, id: NibId) -> Option<usize> {
        Self::table_len(self, id)
    }

    fn truncate_table(&self, id: NibId, len: usize) {
        Self::truncate_table(self, id, len);
    }

    fn take_dirty_entries(&self, id: NibId) -> u64 {
        Self::take_dirty_entries(self, id)
    }

    fn take_len_dirty(&self, id: NibId) -> bool {
        Self::take_len_dirty(self, id)
    }

    fn import_entry(&self, id: NibId, index: usize, data: &[u8]) -> bool {
        Self::import_entry(self, id, index, data)
    }

    fn encode_entry(&self, id: NibId, index: usize, buf: &mut [u8]) -> Option<usize> {
        // round incoming counters down to a window boundary so the stored row
        // only changes once per WINDOW frames from that sender
        if id == NibId::incoming_frame_counters {
            let counters = self.incoming_frame_counters();
            let mut entry = Clone::clone(counters.get(index)?);
            drop(counters);
            entry.incoming_frame_counter = round_down(entry.incoming_frame_counter);
            let mut offset = 0;
            buf.write_with(&mut offset, entry, byte::LE).ok()?;
            return Some(offset);
        }
        Self::export_entry(self, id, index, buf)
    }
}
