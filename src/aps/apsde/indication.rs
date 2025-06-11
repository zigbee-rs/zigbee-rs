use super::status::SecurityStatus;
use crate::aps::types::DstAddrMode;
use crate::aps::types::SrcAddrMode;
use crate::aps::types::{self};
use crate::impl_byte;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ApsdeSapIndicationStatus {
    #[default]
    Success,
    DefragUnsupported,
    DefragDeferred,
}

/// 2.2.4.1.3
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsdeSapIndication {
    dst_addr_mode: DstAddrMode,
    dst_address: u8,
    dst_endpoint: u8,
    src_addr_mode: SrcAddrMode,
    src_address: u64,
    src_endpoint: types::SrcEndpoint,
    profile_id: u16,
    cluster_id: u16,
    asdulength: u8,
    status: ApsdeSapIndicationStatus,
    security_status: SecurityStatus,
    link_quality: u8,
    rx_time: u8,
}
