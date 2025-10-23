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

use core::default;

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

use crate::aps::aib::Aib;
use crate::aps::aib::AibStorage;
use crate::nwk::nib::Nib;
use crate::nwk::nib::NibStorage;

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
    // 3.6.1.6.1.2
    fn rejoin(&self) -> NlmeJoinConfirm;
}

pub struct Nlme {
    pub nib: Nib<NibStorage>,
    pub aib: Aib<AibStorage>,
}

impl Default for Nlme {
    fn default() -> Self {
        let nib_storage = NibStorage::default();
        let aib_storage = AibStorage::default();

        Self::new(nib_storage, aib_storage)
    }
}

impl Nlme {
    pub fn new(nib_storage: NibStorage, aib_storage: AibStorage) -> Self {
        let nib = Nib::new(nib_storage);
        nib.init();

        let aib = Aib::new(aib_storage);
        aib.init();

        Self {
            nib,
            aib,
        }
    }
}

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

    fn join(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm {
        // TODO: validate channel list see 3.2.2.2.2

        // TODO: update neighbor table if join is successful
        //self.nib.set_neighbor_table(value);

        // TODO: start routing (3.6.4.1)
        NlmeJoinConfirm {
            status: NlmeJoinStatus::InvalidRequest,
            network_address: 0u16,
            extended_pan_id: request.extended_pan_id,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        }
    }

    //
    fn rejoin(&self) -> NlmeJoinConfirm {
        // TODO: read extended_pan_id from NIB
        let _extended_pan_id = self.nib.extended_panid();

        let request = NlmeJoinRequest {
            // TODO: set ExtendedPANId parameter to the extended PAN identifier of the known network
            extended_pan_id: 0xf4ce_36c3_781b_fcaeu64,
            rejoin_network: 0x02,
            // TODO: set ScanChannels parameter to 0x00000000
            scan_duration: 0x00,
            // TODO: set the CapabilityInformation appropriately for the node
            security_enabled: false,
        };

        self.join(request)
    }
}

