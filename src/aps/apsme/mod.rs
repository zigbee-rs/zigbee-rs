//! Application Support Sub-Layer Management Entity
//!
//! The APSME shall provide a management service to allow an application to
//! interact with the stack
//!
//! it provices the following services:
//! * Binding management
//! * AIB management
//! * Security
//! * Group management
#![allow(dead_code)]

use super::aib::ApsInformationBase;
use super::binding::ApsBindingTable;
use super::types::Address;
use crate::nwk::nlme::management::NlmeJoinRequest;
use crate::nwk::nlme::management::NlmeJoinStatus;
use crate::nwk::nlme::management::NlmeNetworkDiscoveryRequest;
use crate::nwk::nlme::Nlme;
use crate::nwk::nlme::NlmeSap;

pub mod basemgt;
pub mod groupmgt;
pub mod sap;

pub(crate) struct Apsme {
    pub(crate) supports_binding_table: bool,
    pub(crate) binding_table: ApsBindingTable,
    pub(crate) joined_network: Option<Address>,
    pub(crate) aib: ApsInformationBase,
    pub(crate) nwk: Nlme,
}

impl Apsme {
    pub(crate) fn new() -> Self {
        Self {
            supports_binding_table: true,
            binding_table: ApsBindingTable::new(),
            joined_network: None,
            aib: ApsInformationBase::new(),
            nwk: Nlme::new(),
        }
    }
    fn is_joined(&self) -> bool {
        self.joined_network.is_some()
    }

    pub(crate) fn start_network_discovery(&self) {
        let request = NlmeNetworkDiscoveryRequest {
            scan_channels_list_structure: [0, 0, 0, 0, 0, 0, 0, 0],
            scan_duration: 10u8,
        };
        let confirm = self.nwk.network_discovery(request);

        match confirm.status {
            crate::nwk::nlme::management::NlmeNetworkDiscoveryStatus::Successful => {
                // TODO: return list of available networks
            }
        }
    }

    pub(crate) fn join_network(&self) {
        let request = NlmeJoinRequest {
            extended_pan_id: 0x0015_8D00_01AB_CD12,
            rejoin_network: 0u8,
            scan_duration: 10u8,
            security_enabled: false,
        };
        let confirm = self.nwk.join(request);
        if let NlmeJoinStatus::Success = confirm.status {
            // confirm.extended_pan_id
        } else {
            // TODO: handle errors
        }
    }

    // 2.2.8.2.2 Binding
    fn add_binding(&self, _address: Address) -> Result<(), &'static str> {
        // TODO: fix binding
        // self.binding_table.create_binding_link(address);

        Ok(())
    }

    fn remove_binding(&self, _address: Address) -> Result<(), &'static str> {
        // TODO: update binding table
        // self.binding_table.retain(|addr| addr != &address);

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use basemgt::ApsmeBindRequestStatus;

    use super::*;
    use crate::aps::{apsme::basemgt::ApsmeBindRequest, types::SrcEndpoint};

    // 2.2.4.3.1
    #[test]
    fn bind_request_device_does_not_support_binding_should_fail() {
        // given
        let mut apsme = Apsme::new();
        apsme.supports_binding_table = false;
        let request = ApsmeBindRequest {
            src_address: Address::Extended(0u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };

        // when
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::IllegalRequest);
    }

    // 2.2.4.3.1
    #[test]
    fn bind_request_from_an_unjoined_device_should_fail() {
        // given
        let mut apsme = Apsme::new();
        let request = ApsmeBindRequest {
            src_address: Address::Extended(0u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };

        // when
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::IllegalRequest);
    }

    // 2.2.4.3.1
    #[test]
    fn bind_request_with_full_table_should_fail() {
        // given
        let mut apsme = Apsme::new();
        apsme.joined_network = Some(Address::Extended(10u64));
        for n in 0..265u64 {
            let request = ApsmeBindRequest {
                src_address: Address::Extended(n),
                src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
                cluster_id: 1u16,
                dst_addr_mode: 0u8,
                dst_address: 1u8,
                dst_endpoint: 2u8,
            };
            let _ = apsme.bind_request(request);
        }

        // when
        let request = ApsmeBindRequest {
            src_address: Address::Extended(999u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::TableFull);
    }

    #[test]
    fn bind_request_with_valid_request_should_succeed() {
        // given
        let mut apsme = Apsme::new();
        apsme.joined_network = Some(Address::Extended(10u64));

        // when
        let request = ApsmeBindRequest {
            src_address: Address::Extended(999u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::Success);
    }
}

