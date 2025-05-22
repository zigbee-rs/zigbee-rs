use crate::common::types::ShortAddress;

pub struct RejoinResponse {
    pub network_address: ShortAddress,
    pub status: u8,
}
