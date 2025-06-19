use crate::common::types::ShortAddress;

pub struct RouteRecord<'a> {
    pub relay_count: u8,
    pub relay_list: &'a [ShortAddress],
}
