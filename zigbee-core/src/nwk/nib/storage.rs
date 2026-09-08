//! Flash persistence of the NIB (see [`crate::storage`]).

use byte::BytesExt;

use super::Nib;
use super::NibId;
use crate::storage::PersistentIb;
use crate::storage::round_down;
use crate::storage::round_up;

impl PersistentIb for Nib {
    type Id = NibId;

    const TAG: u16 = 0x0000;
    const NAME: &'static str = "NIB";
    const COUNTER_FIELD: NibId = NibId::security_material_set;
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
        if id != Self::COUNTER_FIELD {
            return self.export_field(id, buf);
        }

        // normalize counters so the stored image only changes when a counter
        // crosses its headroom boundary
        let mut set = Clone::clone(&*self.security_material_set());
        for material in set.iter_mut() {
            material.outgoing_frame_counter = round_up(material.outgoing_frame_counter);
            for entry in material.incoming_frame_counter_set.iter_mut() {
                entry.incoming_frame_counter = round_down(entry.incoming_frame_counter);
            }
        }

        let mut offset = 0;
        buf.write_with(&mut offset, set, byte::LE).ok()?;
        Some(offset)
    }
}
