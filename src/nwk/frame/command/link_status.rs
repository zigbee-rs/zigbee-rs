pub struct LinkStatus<'a> {
    pub entries: &'a [NeighborTableEntry],
}

pub struct NeighborTableEntry {
    pub neighbor_address: u16,
    pub link_quality: u8,
}
