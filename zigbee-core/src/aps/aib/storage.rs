//! Flash persistence of the AIB (see [`crate::storage`]).

use byte::BytesExt;

use super::Aib;
use super::AibId;
use crate::storage::HEADROOM;
use crate::storage::PersistentIb;

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

    fn import_entry(&self, id: AibId, index: usize, data: &[u8]) -> bool {
        Self::import_entry(self, id, index, data)
    }

    fn encode_entry(&self, id: AibId, index: usize, buf: &mut [u8]) -> Option<usize> {
        // the pair's outgoing counter gets the same headroom as the NWK one
        if id == AibId::device_key_pair_set {
            let table = self.device_key_pair_set();
            let mut pair = Clone::clone(table.get(index)?);
            drop(table);
            pair.outgoing_frame_counter = pair.outgoing_frame_counter.saturating_add(HEADROOM);
            let mut offset = 0;
            buf.write_with(&mut offset, pair, byte::LE).ok()?;
            return Some(offset);
        }
        Self::export_entry(self, id, index, buf)
    }
}
