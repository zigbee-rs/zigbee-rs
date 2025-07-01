pub struct LinkPowerDelta<'a> {
    pub entries: &'a [DeltaEntry],
}

pub struct DeltaEntry {
    pub neighbor_address: u16,
    pub delta: i8,
}
