use super::types::TxOptions;
use crate::aps::types::Address;
use crate::aps::types::DstAddrMode;
use crate::aps::types::{self};

/// APSDE Data request
///
/// 2.2.4.1.1
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsdeSapRequest {
    pub(crate) dst_addr_mode: DstAddrMode,
    pub(crate) dst_address: Address,
    pub(crate) dst_endpoint: u8,
    pub(crate) profile_id: u16,
    pub(crate) cluster_id: u16,
    pub(crate) src_endpoint: types::SrcEndpoint,
    pub(crate) asdulength: u8,
    pub(crate) asdu: u8,
    pub(crate) tx_options: TxOptions,
    pub(crate) use_alias: bool,
    pub(crate) alias_src_addr: u16,
    pub(crate) alias_seq_number: u8,
    pub(crate) radius_counter: u8,
}
