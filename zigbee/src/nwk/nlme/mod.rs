//! Network Management Entity
//!
//! The NLME shall provide a management service to allow an application to
//! interact with the stack.
//!
//! it provides:
//! * configuring a new device
//! * starting a network
//! * joining, rejoining and leaving a network
//! * addressing
//! * neighbor discovery
//! * route discovery
//! * reception control
//! * routing
#![allow(dead_code)]

use management::NlmeEdScanConfirm;
use management::NlmeEdScanRequest;
use management::NlmeJoinConfirm;
use management::NlmeJoinRequest;
use management::NlmeJoinStatus;
use management::NlmeNetworkDiscoveryConfirm;
use management::NlmeNetworkDiscoveryRequest;
use management::NlmeNetworkFormationConfirm;
use management::NlmeNetworkFormationRequest;
use management::NlmePermitJoiningConfirm;
use management::NlmePermitJoiningRequest;
use management::NlmeStartRouterConfirm;
use management::NlmeStartRouterRequest;

#[cfg(feature = "mock")]
use mockall::{automock, mock};

/// Network management entity
pub mod management;

/// Network management service - service access point
///
/// 3.2.2
///
/// allows the transport of management commands between the next higher layer
/// and the NLME.
#[cfg_attr(feature = "mock", automock)]
pub trait NlmeSap {
    /// 3.2.2.3
    fn network_discovery(
        &mut self,
        request: NlmeNetworkDiscoveryRequest,
    ) -> NlmeNetworkDiscoveryConfirm;
    /// 3.2.2.5
    fn network_formation(
        &self,
        request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm;
    /// 3.2.2.7
    fn permit_joining(&self, request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm;
    /// 3.2.2.9
    fn start_router(&self, request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm;
    /// 3.2.2.11
    fn ed_scan(&self, request: NlmeEdScanRequest) -> NlmeEdScanConfirm;
    // 3.2.2.13
    fn join(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm;

    fn rejoin(&self) -> NlmeJoinConfirm;
}

#[derive(Clone, Copy)]
pub struct Nlme {}

impl NlmeSap for Nlme {
    fn network_discovery(
        &mut self,
        _request: NlmeNetworkDiscoveryRequest,
    ) -> NlmeNetworkDiscoveryConfirm {
        // TODO: perform an active network scan
        todo!()
    }

    fn network_formation(
        &self,
        _request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm {
        todo!()
    }

    // Permitting Devices to Join a Network
    // Figure 3-39
    fn permit_joining(&self, _request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm {
        NlmePermitJoiningConfirm { status: NlmeJoinStatus::InvalidRequest }
    }

    fn start_router(&self, _request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm {
        todo!()
    }

    fn ed_scan(&self, _request: NlmeEdScanRequest) -> NlmeEdScanConfirm {
        todo!()
    }

    fn join(&self, _request: NlmeJoinRequest) -> NlmeJoinConfirm {
        // TODO: update neighbor table if join is successful
        // TODO: start routing (3.6.4.1)
        NlmeJoinConfirm {
            status: NlmeJoinStatus::InvalidRequest,
            network_address: 0u16,
            extended_pan_id: 0u64,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        }
    }

    fn rejoin(&self) -> NlmeJoinConfirm {
        // TODO: read extended_pan_id from NIB
        let request = NlmeJoinRequest {
            // TODO: set ExtendedPANId parameter to the extended PAN identifier of the known network
            extended_pan_id: 0u64,
            rejoin_network: 0x02,
            // TODO: set ScanChannels parameter to 0x00000000
            scan_duration: 0x00,
            // TODO: set the CapabilityInformation appropriately for the node
            security_enabled: true,
        };

        self.join(request)
    }
}

