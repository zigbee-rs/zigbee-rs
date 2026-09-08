//! Flash persistence of the NIB (see [`crate::storage`]).

use byte::BytesExt;

use super::Nib;
use super::NibId;
use crate::storage::HEADROOM;
use crate::storage::PersistentIb;

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
        // flushing is asynchronous, so store a value the live counter cannot
        // have reached yet: a reset must never hand out a counter that was
        // already transmitted (4.3.4)
        if id == NibId::outgoing_frame_counter {
            let mut offset = 0;
            let bound = self.outgoing_frame_counter().saturating_add(HEADROOM);
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

    fn import_entry(&self, id: NibId, index: usize, data: &[u8]) -> bool {
        Self::import_entry(self, id, index, data)
    }

    fn encode_entry(&self, id: NibId, index: usize, buf: &mut [u8]) -> Option<usize> {
        Self::export_entry(self, id, index, buf)
    }
}
