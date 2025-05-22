use core::ops::Not;

use super::basemgt;
use super::basemgt::ApsmeAddGroupConfirm;
use super::basemgt::ApsmeAddGroupRequest;
use super::basemgt::ApsmeBindConfirm;
use super::basemgt::ApsmeBindRequest;
use super::basemgt::ApsmeBindRequestStatus;
use super::basemgt::ApsmeGetConfirm;
use super::basemgt::ApsmeGetConfirmStatus;
use super::basemgt::ApsmeRemoveAllGroupsConfirm;
use super::basemgt::ApsmeRemoveAllGroupsRequest;
use super::basemgt::ApsmeRemoveGroupConfirm;
use super::basemgt::ApsmeRemoveGroupRequest;
use super::basemgt::ApsmeSetConfirm;
use super::basemgt::ApsmeUnbindConfirm;
use super::basemgt::ApsmeUnbindRequest;
use super::basemgt::ApsmeUnbindRequestStatus;
use super::Apsme;

use crate::aps::aib::AIBAttribute;


/// Application support sub-layer management service - service access point
///
/// 2.2.4.2
///
/// supports the transport of management commands between the NHLE and the APSME
pub trait ApsmeSap {
    /// 2.2.4.3.1 - request to bind two devices together, or to bind a device to
    /// a group
    fn bind_request(&mut self, request: ApsmeBindRequest) -> ApsmeBindConfirm;
    /// 2.2.4.3.3 - request to unbind two devices, or to unbind a device from a
    /// group
    fn unbind_request(&mut self, request: ApsmeUnbindRequest) -> ApsmeUnbindConfirm;
    /// 2.2.4.4.1 - APSME-GET.request
    fn get(&self, attribute: u8) -> ApsmeGetConfirm;
    /// 2.2.4.4.3 - APSME-SET.request
    fn set(&mut self, attribute: AIBAttribute) -> ApsmeSetConfirm;
    /// 2.2.4.5.1 - APSME-ADD-GROUP.request
    fn add_group(&self, request: ApsmeAddGroupRequest) -> ApsmeAddGroupConfirm;
    /// 2.2.4.5.3 - APSME-REMOVE-GROUP.request
    fn remove_group(&self, request: ApsmeRemoveGroupRequest) -> ApsmeRemoveGroupConfirm;
    /// 2.2.4.5.5 - APSME-REMOVE-ALL-GROUPS.request
    fn remove_all_groups(
        &self,
        request: ApsmeRemoveAllGroupsRequest,
    ) -> ApsmeRemoveAllGroupsConfirm;
}
impl ApsmeSap for Apsme {
    /// 2.2.4.3.1 - APSME-BIND.request
    /// request to bind two devices together, or to bind a device to a group
    fn bind_request(&mut self, request: ApsmeBindRequest) -> ApsmeBindConfirm {
        let status = if !self.is_joined() || !self.supports_binding_table {
            ApsmeBindRequestStatus::IllegalRequest
        } else if self.binding_table.is_full() {
            ApsmeBindRequestStatus::TableFull
        } else {
            match self.binding_table.create_binding_link(&request) {
                Ok(_) => ApsmeBindRequestStatus::Success,
                Err(_) => ApsmeBindRequestStatus::IllegalRequest,
            }
        };

        ApsmeBindConfirm {
            status,
            src_address: request.src_address,
            src_endpoint: request.src_endpoint,
            cluster_id: request.cluster_id,
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
        }
    }

    /// 2.2.4.3.3 - request to unbind two devices, or to unbind a device from a
    /// group
    fn unbind_request(&mut self, request: ApsmeUnbindRequest) -> ApsmeUnbindConfirm {
        let status = if self.is_joined().not() {
            ApsmeUnbindRequestStatus::IllegalRequest
        } else {
            let res = self.binding_table.remove_binding_link(&request);
            match res {
                Ok(_) => ApsmeUnbindRequestStatus::Success,
                Err(err) => match err {
                    crate::aps::binding::BindingError::IllegalRequest => {
                        ApsmeUnbindRequestStatus::IllegalRequest
                    }
                    crate::aps::binding::BindingError::InvalidBinding => {
                        ApsmeUnbindRequestStatus::InvalidBinding
                    }
                    _ => ApsmeUnbindRequestStatus::IllegalRequest,
                },
            }
        };

        ApsmeUnbindConfirm {
            status,
            src_address: request.src_address,
            src_endpoint: request.src_endpoint,
            cluster_id: request.cluster_id,
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
        }
    }

    // 2.2.4.4.1 APSME-GET.request
    fn get(&self, identifier: u8) -> ApsmeGetConfirm {
        let attr = self.aib.get_attribute(identifier);
        attr.map_or(ApsmeGetConfirm {
                status: ApsmeGetConfirmStatus::UnsupportedAttribute,
                attribute: identifier,
                attribute_length: 0,
                attribute_value: None,
            }, |attr| ApsmeGetConfirm {
                status: ApsmeGetConfirmStatus::Success,
                attribute: attr.id(),
                attribute_length: attr.length(),
                attribute_value: Some(attr.value()),
            })
    }

    // 2.2.4.4.3 APSME-SET.request
    fn set(&mut self, attribute: AIBAttribute) -> ApsmeSetConfirm {
        let id = attribute.id();
        match self.aib.write_attribute_value(id, attribute) {
            Ok(_) => ApsmeSetConfirm {
                status: basemgt::ApsmeSetConfirmStatus::Success,
                identifier: id,
            },
            Err(_) => todo!(),
        }
    }

    /// 2.2.4.5.1 - APSME-ADD-GROUP.request
    fn add_group(&self, _request: ApsmeAddGroupRequest) -> ApsmeAddGroupConfirm {
        ApsmeAddGroupConfirm {}
    }

    /// 2.2.4.5.3 - APSME-REMOVE-GROUP.request
    fn remove_group(&self, _request: ApsmeRemoveGroupRequest) -> ApsmeRemoveGroupConfirm {
        todo!()
    }

    /// 2.2.4.5.5 - APSME-REMOVE-ALL-GROUPS.request
    fn remove_all_groups(
        &self,
        _request: ApsmeRemoveAllGroupsRequest,
    ) -> ApsmeRemoveAllGroupsConfirm {
        todo!()
    }
}

