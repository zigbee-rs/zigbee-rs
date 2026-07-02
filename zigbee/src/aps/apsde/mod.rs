//! Application Support Sub-Layer Data Entity
//!
//! The APSDE provides the data transmission service for application objects
//! and the ZDO. It generates the APS PDU and hands it to the NWK layer for
//! encryption and routing.
//!
//! See Zigbee R22 §2.2.4.1.
#![allow(dead_code)]

use super::types::Address;
use super::types::DstAddrMode;
use super::types::SrcAddrMode;
use super::types::TxOptions;
use crate::aps::types;

// 2.2.4.1.1
#[derive(Debug, Clone, PartialEq)]
pub struct ApsdeSapRequest<'a> {
    pub dst_addr_mode: DstAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
    pub profile_id: u16,
    pub cluster_id: u16,
    pub src_endpoint: types::SrcEndpoint,
    pub asdu: &'a [u8],
    pub tx_options: TxOptions,
    pub use_alias: bool,
    pub alias_src_addr: u16,
    pub alias_seq_number: u8,
    pub radius_counter: u8,
}

impl Default for ApsdeSapRequest<'_> {
    fn default() -> Self {
        Self {
            dst_addr_mode: DstAddrMode::default(),
            dst_address: Address::default(),
            dst_endpoint: 0,
            profile_id: 0,
            cluster_id: 0,
            src_endpoint: types::SrcEndpoint::default(),
            asdu: &[],
            tx_options: TxOptions::default(),
            use_alias: false,
            alias_src_addr: 0,
            alias_seq_number: 0,
            radius_counter: 0,
        }
    }
}

/// The status of the corresponding request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    /// addressing mode not implemented (MVP: only Network/Short is supported)
    Unsupported,
}

// 2.2.4.1.2
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ApsdeSapConfirm {
    pub dst_addr_mode: DstAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
    pub src_endpoint: types::SrcEndpoint,
    pub status: ApsdeSapConfirmStatus,
    pub tx_time: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ApsdeSapIndicationStatus {
    #[default]
    Success,
    DefragUnsupported,
    DefragDeferred,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SecurityStatus {
    #[default]
    Unsecured,
    SecuredNwkKey,
    SecuredLinkKey,
}

// 2.2.4.1.3
#[derive(Debug, Clone, PartialEq)]
pub struct ApsdeSapIndication<'a> {
    pub dst_addr_mode: DstAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
    pub src_addr_mode: SrcAddrMode,
    pub src_address: Address,
    pub src_endpoint: types::SrcEndpoint,
    pub profile_id: u16,
    pub cluster_id: u16,
    pub asdu: &'a [u8],
    pub status: ApsdeSapIndicationStatus,
    pub security_status: SecurityStatus,
    pub link_quality: u8,
    pub rx_time: u8,
}
