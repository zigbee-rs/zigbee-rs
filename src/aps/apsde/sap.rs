use super::request::ApsdeSapRequest;
use super::status::ApsdeSapConfirmStatus;
use super::Apsde;
use crate::aps::types::Address;
use crate::aps::types::DstAddrMode;
use crate::aps::types::{self};

/// Application support sub-layer data entity – service access point
///
/// 2.2.4.1.1
///
/// Interface between the NWK (Network) layer and the APL (Application) layer
/// through a general set of services for use by both the ZDO (device object)
/// and the application.
pub trait ApsdeSap {
    /// 2.2.4.1.1 - APSDE-DATA.request  
    /// Requests the transfer of a NHLE PDU from a local NHLE to one or more
    /// peer NHLE entities
    fn data_request(&self, request: ApsdeSapRequest) -> ApsdeSapConfirm;
}

impl ApsdeSap for Apsde {
    /// 2.2.4.1.1 - APSDE-DATA.request  
    fn data_request(&self, request: ApsdeSapRequest) -> ApsdeSapConfirm {
        let status = if request.dst_addr_mode == DstAddrMode::None && self.supports_binding_table {
            // TODO: search binding table with endpoint and cluster identifiers
            // request.src_endpoint.value
            // request.cluster_id
            if false {
                // TODO if no binding table entries found
                ApsdeSapConfirmStatus::NoBoundDevice
            } else {
                // TODO: fix
                ApsdeSapConfirmStatus::Success
            }
        } else {
            // TODO: fix
            ApsdeSapConfirmStatus::NoAck
        };
        ApsdeSapConfirm {
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
            src_endpoint: request.src_endpoint,
            status,
            tx_time: 0,
        }
    }
}

// 2.2.4.1.2
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsdeSapConfirm {
    pub dst_addr_mode: DstAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
    pub src_endpoint: types::SrcEndpoint,
    pub status: ApsdeSapConfirmStatus,
    pub tx_time: u8,
}
