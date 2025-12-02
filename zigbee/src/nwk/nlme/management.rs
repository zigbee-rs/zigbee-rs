use zigbee_mac::mlme::PanDescriptor;
use zigbee_mac::BeaconOrder;
use zigbee_mac::SuperframeOrder;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

/// 3.2.2.4 - NLME-NETWORK-DISCOVERY.confirm
#[derive(Debug)]
pub struct NlmeNetworkDiscoveryConfirm {
    #[cfg(feature = "alloc")]
    pub network_descriptor: alloc::vec::Vec<NetworkDescriptor>,
    #[cfg(not(feature = "alloc"))]
    pub network_descriptor:
        heapless::Vec<NetworkDescriptor, { zigbee_mac::mlme::MAX_IEEE802154_CHANNELS }>,
}

/// Network descriptor
#[derive(Debug)]
pub struct NetworkDescriptor {
    /// 64-bit PAN identifier
    pub extended_pan_id: IeeeAddress,
    /// 16-bit PAN identifier
    pub pan_id: ShortAddress,
    /// update ID from the NIB
    pub update_id: u8,
    /// current logical channel
    pub logical_channel: u8,
    /// A zigbee stack profile
    pub stack_profile: u8,
    /// version of the ZigBee protocol in use
    pub zigbee_version: u8,
    /// specifies how often the MAC sub-layer beacon is to be transmitted
    pub beacon_order: BeaconOrder,
    /// for beacon oriented networks
    pub superframe_order: SuperframeOrder,
    /// indicates that at least one ZigBee router or network currently permits
    /// joineng
    pub permit_joining: bool,
    /// set to TRUE if the device is capable of accepting join requests from
    /// router-capable devices
    pub router_capacity: bool,
    /// set to TRUE if the device is capable of accepting join requests from end
    /// devices
    pub end_device_capacity: bool,
}

impl From<PanDescriptor> for NetworkDescriptor {
    fn from(pd: PanDescriptor) -> Self {
        Self {
            extended_pan_id: pd.zigbee_beacon.extended_pan_id,
            pan_id: pd.coord_pan_id,
            update_id: pd.zigbee_beacon.update_id,
            logical_channel: pd.channel,
            stack_profile: pd.zigbee_beacon.stack_profile.stack_profile(),
            zigbee_version: pd.zigbee_beacon.stack_profile.protocol_version(),
            beacon_order: pd.superframe_spec.beacon_order,
            superframe_order: pd.superframe_spec.superframe_order,
            permit_joining: pd.superframe_spec.association_permit,
            router_capacity: pd.zigbee_beacon.stack_profile.router_capacity(),
            end_device_capacity: pd.zigbee_beacon.stack_profile.end_device_capacity(),
        }
    }
}

/// 3.2.2.5 - NLME-NETWORK-FORMATION.request
pub struct NlmeNetworkFormationRequest {}
/// 3.2.2.6 - NLME-NETWORK-FORMATION.confirm
pub struct NlmeNetworkFormationConfirm {}

/// 3.2.2.7 - NLME-PERMIT-JOINING.request
pub struct NlmePermitJoiningRequest {
    pub permit_duration: u8,
}
/// 3.2.2.8 - NLME-PERMIT-JOINING.confirm
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NlmePermitJoiningConfirm {
    pub status: NlmeJoinStatus,
}
/// 3.2.2.9 - NLME-START-ROUTER.request
pub struct NlmeStartRouterRequest {}
/// 3.2.2.10 - NLME-START-ROUTER.confirm
pub struct NlmeStartRouterConfirm {}
/// 3.2.2.11 - NLME-ED-SCAN.request
pub struct NlmeEdScanRequest {}
/// 3.2.2.12 - NLME-ED-SCAN.confirm
pub struct NlmeEdScanConfirm {}
/// 3.2.2.13 - NLME-JOIN.request
pub struct NlmeJoinRequest {
    pub extended_pan_id: u64,
    pub rejoin_network: u8,
    // ScanChannelsListStructure
    pub scan_duration: u8,
    // CapabilityInformation
    pub security_enabled: bool,
}
/// 3.2.2.14 - NLME-JOIN.indication
pub struct NlmeJoinIndication {
    pub(crate) network_address: u16,
    pub(crate) extended_address: u64,
    //CapabilityInformation
    pub(crate) rejoin_network: u8,
    pub(crate) secure_rejoin: bool,
}
/// 3.2.2.15 - NLME-JOIN.confirm
pub struct NlmeJoinConfirm {
    pub status: NlmeJoinStatus,
    pub network_address: u16,
    pub extended_pan_id: u64,
    // Channel List Structure
    pub enhanced_beacon_type: bool,
    pub mac_interface_index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NlmeJoinStatus {
    Success,
    InvalidRequest,
    NotPermitted,
    NoNetworks,
    // TODO: add more from 3.2.2.13.3
}

/// 3.2.2.16 - NLME-DIRECT-JOIN.request
pub struct NlmeDirectJoinRequest {}
/// 3.2.2.17 - NLME-DIRECT-JOIN.confirm
pub struct NlmeDirectJoinConfirm {}

/// 3.2.2.18 - NLME-LEAVE.request
pub struct NlmeLeaveRequest {}
/// 3.2.2.19 - NLME-LEAVE.indication
pub struct NlmeLeaveIndication {}
/// 3.2.2.20 - NLME-LEAVE.confirm
pub struct NlmeLeaveConfirm {}

/// 3.2.2.21 - NLME-RESET.request
pub struct NlmeResetRequest {}
/// 3.2.2.22 - NLME-RESET.confirm
pub struct NlmeResetConfirm {}
