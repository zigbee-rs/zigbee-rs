#![allow(dead_code)]
#![allow(missing_docs)]
//!
//! 2.2.4.4 Information Base Maintenance
//! This set of primitives defines how the next higher layer of a device can
//! read and write attributes in the AIB
use crate::aps::aib::AIBAttribute;
use crate::aps::aib::AIBAttributeValue;
use crate::aps::types::Address;
use crate::aps::types::{self};
use crate::impl_byte;

type DstAddrMode = u8;

/// 2.2.4.3.1 - APSME-BIND.request
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeBindRequest {
    pub src_address: Address,
    pub src_endpoint: types::SrcEndpoint,
    pub cluster_id: u16,
    pub dst_addr_mode: DstAddrMode,
    pub dst_address: u8,
    pub dst_endpoint: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ApsmeBindRequestStatus {
    #[default]
    Success,
    IllegalRequest,
    TableFull,
    NotSupported,
}

/// 2.2.4.3.2 - APSME-BIND.confirm
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeBindConfirm {
    pub(crate) status: ApsmeBindRequestStatus,
    pub src_address: Address,
    pub src_endpoint: types::SrcEndpoint,
    pub cluster_id: u16,
    pub dst_addr_mode: DstAddrMode,
    pub dst_address: u8,
    pub dst_endpoint: u8,
}

/// 2.2.4.3.3 - APSME-UNBIND.request
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeUnbindRequest {
    pub(crate) src_address: Address,
    pub(crate) src_endpoint: types::SrcEndpoint,
    pub(crate) cluster_id: u16,
    pub(crate) dst_addr_mode: DstAddrMode,
    pub(crate) dst_address: u8,
    pub(crate) dst_endpoint: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ApsmeUnbindRequestStatus {
    #[default]
    Success,
    IllegalRequest,
    InvalidBinding,
}

/// 2.2.4.3.4 - APSME-UNBIND.confirm
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeUnbindConfirm {
    pub(crate) status: ApsmeUnbindRequestStatus,
    pub(crate) src_address: Address,
    pub(crate) src_endpoint: types::SrcEndpoint,
    pub(crate) cluster_id: u16,
    pub(crate) dst_addr_mode: DstAddrMode,
    pub(crate) dst_address: u8,
    pub(crate) dst_endpoint: u8,
}

/// 2.2.4.4.1 - APSME-GET.request
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeGetRequest {
    attribute: AIBAttribute,
}

/// 2.2.4.4.2 - APSME-GET.confirm
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeGetConfirm {
    pub(crate) status: ApsmeGetConfirmStatus,
    pub(crate) attribute: u8,
    pub(crate) attribute_length: u8,
    pub(crate) attribute_value: Option<AIBAttributeValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ApsmeGetConfirmStatus {
    #[default]
    Success,
    UnsupportedAttribute,
}

/// 2.2.4.4.3 - APSME-SET.request
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeSetRequest {
    attribute: AIBAttribute,
    attribute_length: u8,
    attribute_value: AIBAttributeValue,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ApsmeSetConfirmStatus {
    #[default]
    Success,
    InvalidParameter,
    UnsupportedAttribute,
}

/// 2.2.4.4.4 - APSME-SET.confirm
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeSetConfirm {
    pub(crate) status: ApsmeSetConfirmStatus,
    pub(crate) identifier: u8,
}

/// 2.2.4.5.1 - APSME-ADD-GROUP.request
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeAddGroupRequest {}

/// 2.2.4.5.2 - APSME-ADD-GROUP.confirm
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeAddGroupConfirm {}

/// 2.2.4.5.3 - APSME-REMOVE-GROUP.request
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveGroupRequest {}

/// 2.2.4.5.3 - APSME-REMOVE-GROUP.request
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveGroupConfirm {}

/// 2.2.4.5.5 - APSME-REMOVE-ALL-GROUPS.request
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveAllGroupsRequest {}

/// 2.2.4.5.6 - APSME-REMOVE-ALL-GROUPs.request
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveAllGroupsConfirm {}
