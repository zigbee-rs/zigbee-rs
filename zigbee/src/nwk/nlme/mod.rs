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

use embedded_storage::Storage;
use management::NlmeEdScanConfirm;
use management::NlmeEdScanRequest;
use management::NlmeJoinConfirm;
use management::NlmeJoinRequest;
use management::NlmeJoinStatus;
use management::NlmeNetworkDiscoveryConfirm;
use management::NlmeNetworkFormationConfirm;
use management::NlmeNetworkFormationRequest;
use management::NlmePermitJoiningConfirm;
use management::NlmePermitJoiningRequest;
use management::NlmeStartRouterConfirm;
use management::NlmeStartRouterRequest;
#[cfg(feature = "mock")]
use mockall::automock;
#[cfg(feature = "mock")]
use mockall::mock;
use thiserror::Error;
use zigbee_mac::mlme::MacError;
use zigbee_mac::mlme::Mlme;
use zigbee_mac::mlme::ScanType;

use crate::nwk::nib;
use crate::nwk::nib::Nib;
use crate::nwk::nib::NibStorage;
use crate::nwk::nlme::management::NetworkDescriptor;

/// Network management entity
pub mod management;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("mac error")]
    MacError(#[from] MacError),
}

/// Network management service - service access point
///
/// 3.2.2
///
/// allows the transport of management commands between the next higher layer
/// and the NLME.
#[cfg_attr(feature = "mock", automock)]
pub trait NlmeSap {
    /// 3.2.2.3
    async fn network_discovery<C: Iterator<Item = u8> + 'static>(
        &mut self,
        channels: C,
        duration: u8,
    ) -> Result<NlmeNetworkDiscoveryConfirm, NetworkError>;
    /// 3.2.2.5
    async fn network_formation(
        &self,
        request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm;
    /// 3.2.2.7
    async fn permit_joining(&self, request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm;
    /// 3.2.2.9
    async fn start_router(&self, request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm;
    /// 3.2.2.11
    async fn ed_scan(&self, request: NlmeEdScanRequest) -> NlmeEdScanConfirm;
    // 3.2.2.13
    async fn join(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm;

    async fn rejoin(&self) -> NlmeJoinConfirm;
}

pub struct Nlme<S, M> {
    nib: Nib<S>,
    mac: M,
}

impl<S, M> Nlme<S, M>
where
    S: Storage,
    M: Mlme,
{
    pub fn new(storage: S, mac: M) -> Self {
        let nib = Nib::new(storage);
        Self { nib, mac }
    }
}

impl<S, M> NlmeSap for Nlme<S, M>
where
    M: Mlme,
{
    async fn network_discovery<C: Iterator<Item = u8>>(
        &mut self,
        channels: C,
        duration: u8,
    ) -> Result<NlmeNetworkDiscoveryConfirm, NetworkError> {
        let scan_result = self
            .mac
            .scan_network(ScanType::Active, channels, duration)
            .await?;

        let network_descriptor = scan_result
            .pan_descriptor
            .into_iter()
            .map(From::from)
            .collect();

        Ok(NlmeNetworkDiscoveryConfirm { network_descriptor })
    }

    async fn network_formation(
        &self,
        _request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm {
        todo!()
    }

    // Permitting Devices to Join a Network
    // Figure 3-39
    async fn permit_joining(&self, _request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm {
        NlmePermitJoiningConfirm {
            status: NlmeJoinStatus::InvalidRequest,
        }
    }

    async fn start_router(&self, _request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm {
        todo!()
    }

    async fn ed_scan(&self, _request: NlmeEdScanRequest) -> NlmeEdScanConfirm {
        todo!()
    }

    async fn join(&self, _request: NlmeJoinRequest) -> NlmeJoinConfirm {
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

    async fn rejoin(&self) -> NlmeJoinConfirm {
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

        self.join(request).await
    }
}
