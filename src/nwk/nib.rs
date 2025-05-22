//! NWK information base
//!
//! See Section 3.5.
#![allow(dead_code)]

use core::mem;

use heapless::FnvIndexMap;
use heapless::Vec;

use crate::common::types::IeeeAddress;
use crate::common::types::ShortAddress;
use crate::security::frame::SecurityLevel;

/// Zigbee device type.
#[derive(Debug)]
#[repr(u8)]
pub enum DeviceType {
    /// Zigbee coordinator
    Coordinator = 0x00,
    /// Zigbee router
    Router = 0x01,
    /// Zigbee end device
    EndDevice = 0x02,
}

/// See Section 3.5.1.
const NWKC_COORDINATOR_CAPABLE: bool = true;
const NWKC_DEFAULT_SECURITY_LEVEL: u8 = 0x00; // defined in stack profile
const NWKC_MIN_HEADER_OVERHEAD: u8 = 0x08;
const NWKC_PROTOCOL_VERSION: u8 = 0x02;
const NWKC_WAIT_BEFORE_VALIDATION: u32 = 0x9c40;
const NWKC_ROUTE_DISCOVERY_TIME: u32 = 0x4c4b4;
const NWKC_MAX_BROADCAST_JITTER: u32 = 0x7d0;
const NWKC_INITIAL_RREQ_RETRIES: u8 = 0x03;
const NWKC_RREQ_RETRIES: u8 = 0x02;
const NWKC_RREQ_RETRY_INTERVAL: u32 = 0x1f02;
const NWKC_MIN_RREQ_JITTER: u32 = 0x3f;
const NWKC_MAX_RREQ_JITTER: u32 = 0xfa0;
const NWKC_MAC_FRAME_OVERHEAD: u8 = 0x0b;

// implementation specific

// 1 for end device
const MAX_NEIGBOUR_TABLE: usize = 16;
// 0 for end devices
const MAX_ROUTE_TABLE: usize = 8;
const MAX_BROADCAST_TRANSACTION_TABLE: usize = 4;
const MAX_GROUP_ID_TABLE: usize = 4;
// 0 for end devices
const MAX_ROUTE_RECORD_TABLE: usize = 8;
const MAX_NWK_ADDRESS_MAP: usize = 16;
const MAX_MAC_INTERFACE_TABLE: usize = 1;

/// Network Information Base.
///
/// See Section 3.5.2.
#[derive(Debug, Default)]
pub(crate) struct Nib {
    // A sequence number used to identify outgoing frames
    sequence_number: u8,
    // defined in stack profile
    passive_ack_timeout: u32,
    // 0x03
    max_broadcast_retries: u8,
    // defined in stack profile
    max_children: u8,
    // defined in stack profile
    max_depth: u8,
    // defined in stack profile
    max_routers: u8,
    neighbor_table: Vec<NwkNeighbor, MAX_NEIGBOUR_TABLE>,
    // defined in stack profile
    network_broadcast_delivery_time: u32,
    // 0x00
    report_constant_cost: u8,
    route_table: Vec<NwkRoute, MAX_ROUTE_TABLE>,
    // false
    sym_link: bool,
    // 0x0
    capability_information: CapabilityInformation,
    // 0x0
    addr_alloc: u8,
    // true
    use_tree_routing: bool,
    // default: 0x0000
    manager_addr: u16,
    // default: 0x0c
    max_source_route: u8,
    // 0x00
    update_id: u8,
    // 0x01f4
    transaction_persistence_time: u16,
    // 0xffff
    network_address: ShortAddress,
    stack_profile: u8,
    broadcast_transaction_table: Vec<TransactionRecord, MAX_BROADCAST_TRANSACTION_TABLE>,
    group_idtable: Vec<u16, MAX_GROUP_ID_TABLE>,
    extended_panid: IeeeAddress,
    // true
    use_multicast: bool,
    route_record_table: Vec<RouteRecord, MAX_ROUTE_RECORD_TABLE>,
    is_concentrator: bool,
    concentrator_radius: u8,
    concentrator_discovery_time: u8,
    security_level: SecurityLevel,
    security_material_set: u8,
    active_key_seq_number: u8,
    all_fresh: u8,
    // 0x0f
    link_status_period: u8,
    // 0x03
    router_age_limit: u8,
    // true
    unique_addr: bool,
    address_map: FnvIndexMap<IeeeAddress, ShortAddress, MAX_NWK_ADDRESS_MAP>,
    time_stamp: bool,
    panid: ShortAddress,
    tx_total: u16,
    // true
    leave_request_allowed: bool,
    parent_information: u8,
    // 0x08
    end_device_timeout_default: u8,
    // true
    leave_request_without_rejoin_allowed: bool,
    ieee_address: IeeeAddress,
    mac_interface_table: Vec<MacInterface, MAX_MAC_INTERFACE_TABLE>,
}

impl Nib {
    pub(crate) fn init() -> Self {
        Self {
            max_broadcast_retries: 0x03,
            use_tree_routing: true,
            max_source_route: 0x0c,
            transaction_persistence_time: 0x01f4,
            network_address: ShortAddress(0xffff),
            use_multicast: true,
            link_status_period: 0x0f,
            router_age_limit: 0x03,
            unique_addr: true,
            end_device_timeout_default: 0x08,
            leave_request_without_rejoin_allowed: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct CapabilityInformation(u8);

#[derive(Debug)]
pub(crate) struct NwkNeighbor {
    extended_address: IeeeAddress,
    network_address: ShortAddress,
    device_type: DeviceType,
    rx_on_when_idle: bool,
    end_device_configuration: u16,
    timeout_counter: u32,
    device_timeout: u32,
    relationship: u8,
    transmit_failure: u8,
    lqi: u8,
    outgoing_cost: Option<u8>,
    age: Option<u8>,
    incoming_beacon_timestamp: Option<u8>,
    beacon_transmission_time: Option<u8>,
    keepalive_received: bool,
    mac_interface_index: u8,
    mac_unicast_bytes_transmitted: Option<u32>,
    mac_unicast_bytes_received: Option<u32>,
}

/// See Table 3-67.
#[derive(Debug)]
#[repr(u8)]
pub(crate) enum RouteStatus {
    Active,
    DiscoveryUnderway,
    DiscoveryFailed,
    Inavtive,
    ValidationUnderway,
    Reserved,
}

/// See Table 3-66.
#[derive(Debug)]
pub(crate) struct NwkRoute {
    pub destination_address: ShortAddress,
    pub next_hop_address: ShortAddress,
    status: u8,
}

impl NwkRoute {
    pub(crate) fn status(&self) -> RouteStatus {
        let status = self.status & 0b111;
        if status > 0x4 {
            RouteStatus::Reserved
        } else {
            // SAFETY: any status <= 0x4 is a valid RouteStatus
            unsafe { mem::transmute::<u8, RouteStatus>(status) }
        }
    }

    /// A flag indicating that the destination indicated by this address does
    /// not store source routes.
    pub(crate) fn no_route_cache(&self) -> bool {
        (self.status >> 3) & 0b1 != 0
    }

    /// A flag indicating that the destination is a concentrator that issued a
    /// many-to-one route request.
    pub(crate) fn many_to_one(&self) -> bool {
        (self.status >> 4) & 0b1 != 0
    }

    /// A flag indicating that a route record command frame should be sent to
    /// the destination prior to the next data packet.
    pub(crate) fn route_record_required(&self) -> bool {
        (self.status >> 5) & 0b1 != 0
    }

    /// A flag indicating that the destination address is a Group ID.
    pub(crate) fn group_id(&self) -> bool {
        (self.status >> 6) & 0b1 != 0
    }
}

/// See Table 3-70.
#[derive(Debug)]
pub(crate) struct TransactionRecord {
    pub source_address: ShortAddress,
    pub sequence_number: u8,
    pub expiration_time: u8,
}

/// See Table 3-59.
#[derive(Debug)]
pub(crate) struct RouteRecord {
    pub network_address: ShortAddress,
    pub relay_count: u16,
    pub path: Vec<ShortAddress, 16>,
}

/// See Table 3-61.
#[derive(Debug)]
pub(crate) struct MacInterface {}
