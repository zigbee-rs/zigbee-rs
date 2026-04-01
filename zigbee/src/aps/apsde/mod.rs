//! Application Support Sub-Layer Data Entity
//!
//! The APSDE shall provide a data service to the network layer and both ZDO and
//! application objects to enable the transport of application PDUs between two
//! or more devices.
//!
//! it will provide:
//! * Generation of the application level PDU (APDU)
//! * Binding
//! * Group address filtering
//! * Reliable transport
//! * Duplicate rejection
//! * Fragmentation
#![allow(dead_code)]
use byte::BytesExt;

use super::types::Address;
use super::types::DstAddrMode;
use super::types::SrcAddrMode;
use super::types::TxOptions;
use crate::aps::apdu::frame::frame_control::DeliveryMode;
use crate::aps::apdu::frame::frame_control::FrameControl;
use crate::aps::apdu::frame::frame_control::FrameType;
use crate::aps::apdu::frame::header::Header;
use crate::aps::types;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::NlmeSap;

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

#[derive(Debug, Clone, Default, PartialEq)]
struct Apsde {
    pub(crate) supports_binding_table: bool,
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

// 2.2.4.1.1
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsdeSapRequest {
    dst_addr_mode: DstAddrMode,
    dst_address: Address,
    dst_endpoint: u8,
    profile_id: u16,
    cluster_id: u16,
    src_endpoint: types::SrcEndpoint,
    asdulength: u8,
    asdu: u8,
    tx_options: TxOptions,
    use_alias: bool,
    alias_src_addr: u16,
    alias_seq_number: u8,
    radius_counter: u8,
}

/// The status of the corresponding request.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ApsdeSapConfirmStatus {
    /// indicating that the request to transmit was successful
    #[default]
    Success,
    /// No corresponding 16-bit NKW address found
    NoShortAddress,
    /// No binding table entries found with the respectively SrcEndpoint and
    /// ClusterId parameter
    NoBoundDevice,
    /// the security processing failed
    SecurityFail,
    /// one or more APS acknowledgements were not correctly received
    NoAck,
    /// ASDU to be transmitted is larger than will fit in a single frame and
    /// fragmentation is not possible
    AsduTooLong,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ApsdeSapIndicationStatus {
    #[default]
    Success,
    DefragUnsupported,
    DefragDeferred,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SecurityStatus {
    #[default]
    Unsecured,
    SecuredNwkKey,
    SecuredLinkKey,
}

// 2.2.4.1.3
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

/// Broadcast an APS data frame to all devices (§2.2.5.1).
///
/// Builds an APS header with the given parameters, appends `payload`,
/// and hands the APDU to the NWK layer via [`NlmeSap::broadcast_data`].
pub async fn broadcast_data<T: NlmeSap>(
    nlme: &mut T,
    aps_counter: &mut u8,
    dst_endpoint: u8,
    cluster_id: u16,
    profile_id: u16,
    src_endpoint: u8,
    payload: &[u8],
) -> Result<(), NetworkError> {
    *aps_counter = aps_counter.wrapping_add(1);
    let counter = *aps_counter;

    let frame_control = FrameControl::default()
        .set_frame_type(FrameType::Data)
        .set_delivery_mode(DeliveryMode::Broadcast);

    let header = Header {
        frame_control,
        destination_endpoint: Some(dst_endpoint),
        group_address: None,
        cluster_id: Some(cluster_id),
        profile_id: Some(profile_id),
        source_endpoint: Some(src_endpoint),
        counter,
        extended_header: None,
    };

    let mut buf = [0u8; 100];
    let offset = &mut 0;
    buf.write_with(offset, header, ())?;

    let hdr_len = *offset;
    let payload_len = payload.len().min(buf.len() - hdr_len);
    buf[hdr_len..hdr_len + payload_len].copy_from_slice(&payload[..payload_len]);

    nlme.broadcast_data(&buf[..hdr_len + payload_len]).await
}
