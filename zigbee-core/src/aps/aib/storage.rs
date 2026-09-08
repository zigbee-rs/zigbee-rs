//! Flash persistence of the AIB (see [`crate::storage`]).

use byte::BytesExt;

use super::Aib;
use super::AibId;
use crate::storage::PersistentIb;
use crate::storage::round_down;
use crate::storage::round_up;

impl PersistentIb for Aib {
    type Id = AibId;

    const TAG: u16 = 0x0100;
    const NAME: &'static str = "AIB";
    const COUNTER_FIELD: AibId = AibId::device_key_pair_set;
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
        if id != Self::COUNTER_FIELD {
            return self.export_field(id, buf);
        }

        // normalize counters so the stored image only changes when a counter
        // crosses its headroom boundary
        let mut set = Clone::clone(&*self.device_key_pair_set());
        for pair in set.iter_mut() {
            pair.outgoing_frame_counter = round_up(pair.outgoing_frame_counter);
            pair.incoming_frame_counter = round_down(pair.incoming_frame_counter);
        }

        let mut offset = 0;
        buf.write_with(&mut offset, set, byte::LE).ok()?;
        Some(offset)
    }
}
