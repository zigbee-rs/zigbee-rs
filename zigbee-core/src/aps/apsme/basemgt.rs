//! APSME primitive types for binding, group management, and AIB access
//! (2.2.4.3 - 2.2.4.5).
#![allow(dead_code)]
#![allow(missing_docs)]
use zigbee_types::IeeeAddress;

use crate::aps::binding::Binding;
use crate::aps::binding::BindingAddrMode;
use crate::aps::types::Address;
use crate::aps::types::{self};

/// 2.2.4.3.1 - APSME-BIND.request
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeBindRequest {
    pub src_address: Address,
    pub src_endpoint: types::SrcEndpoint,
    pub cluster_id: u16,
    pub dst_addr_mode: BindingAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
}

impl ApsmeBindRequest {
    /// The binding table entry this request describes, or `None` when the
    /// addressing mode and address do not agree (2.2.4.3.1).
    pub(crate) fn binding(&self) -> Option<Binding> {
        match (self.dst_addr_mode, self.dst_address) {
            (BindingAddrMode::Device, Address::Extended(ieee)) => Some(Binding::device(
                self.src_endpoint.value,
                self.cluster_id,
                IeeeAddress(ieee),
                self.dst_endpoint,
            )),
            (BindingAddrMode::Group, Address::Group(group)) => Some(Binding::group(
                self.src_endpoint.value,
                self.cluster_id,
                group,
            )),
            _ => None,
        }
    }
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
    pub dst_addr_mode: BindingAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
}

/// 2.2.4.3.3 - APSME-UNBIND.request
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApsmeUnbindRequest {
    pub src_address: Address,
    pub src_endpoint: types::SrcEndpoint,
    pub cluster_id: u16,
    pub dst_addr_mode: BindingAddrMode,
    pub dst_address: Address,
    pub dst_endpoint: u8,
}

impl ApsmeUnbindRequest {
    /// The binding table entry this request names (2.2.4.3.3).
    pub(crate) fn binding(&self) -> Option<Binding> {
        ApsmeBindRequest {
            src_address: self.src_address,
            src_endpoint: self.src_endpoint,
            cluster_id: self.cluster_id,
            dst_addr_mode: self.dst_addr_mode,
            dst_address: self.dst_address,
            dst_endpoint: self.dst_endpoint,
        }
        .binding()
    }
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
    pub(crate) dst_addr_mode: BindingAddrMode,
    pub(crate) dst_address: Address,
    pub(crate) dst_endpoint: u8,
}

/// 2.2.4.4.2 - APSME-GET.confirm
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeGetConfirm {
    pub(crate) status: ApsmeGetConfirmStatus,
    pub(crate) attribute: u8,
    pub(crate) attribute_length: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ApsmeGetConfirmStatus {
    #[default]
    Success,
    UnsupportedAttribute,
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
/// 2.2.4.5.4 - APSME-REMOVE-GROUP.confirm
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveGroupConfirm {}

/// 2.2.4.5.5 - APSME-REMOVE-ALL-GROUPS.request
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveAllGroupsRequest {}
/// 2.2.4.5.6 - APSME-REMOVE-ALL-GROUPS.confirm
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApsmeRemoveAllGroupsConfirm {}
