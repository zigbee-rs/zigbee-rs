use crate::internal::types::ShortAddress;

pub struct RejoinResponse {
    pub network_address: ShortAddress,
    pub status: u8,
}
