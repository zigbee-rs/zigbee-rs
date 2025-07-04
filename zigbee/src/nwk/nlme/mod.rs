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
use management::NlmeNetworkDiscoveryConfirm;
use management::NlmeNetworkDiscoveryRequest;
use management::NlmeNetworkFormationConfirm;
use management::NlmeNetworkFormationRequest;
use management::NlmePermitJoiningConfirm;
use management::NlmePermitJoiningRequest;
use management::NlmeStartRouterConfirm;
use management::NlmeStartRouterRequest;

/// Network management entity
pub mod management;

/// Network management service - service access point
///
/// 3.2.2
///
/// allows the transport of management commands between the next higher layer
/// and the NLME.
pub trait NlmeSap {
    /// 3.2.2.3
    fn network_discovery(
        &self,
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
}

pub(crate) struct Nlme {}

impl Nlme {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl NlmeSap for Nlme {
    fn network_discovery(
        &self,
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

    fn permit_joining(&self, _request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm {
        todo!()
    }

    fn start_router(&self, _request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm {
        todo!()
    }

    fn ed_scan(&self, _request: NlmeEdScanRequest) -> NlmeEdScanConfirm {
        todo!()
    }

    fn join(&self, _request: NlmeJoinRequest) -> NlmeJoinConfirm {
        // Figure 3-39
        // TODO: update neighbor table if join is successful
        // TODO: start routing (3.6.4.1)
        NlmeJoinConfirm {
            status: management::NlmeJoinStatus::InvalidRequest,
            network_address: 0u16,
            extended_pan_id: 0u64,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        }
    }
}
