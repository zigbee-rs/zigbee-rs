//! NWK information base
//!
//! See Section 3.5.
use core::mem;
use core::ops::Deref;
use core::ops::DerefMut;

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use heapless::Vec;
use heapless::index_map::FnvIndexMap;
use zigbee_macros::construct_ib;
use zigbee_macros::impl_byte;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::StorageVec;

use crate::security::frame::SecurityLevel;

impl_byte! {
    #[tag(u8)]
    /// Zigbee device type.
    #[derive(Debug, Clone)]
    pub enum DeviceType {
        /// Zigbee coordinator
        Coordinator = 0x00,
        /// Zigbee router
        Router = 0x01,
        /// Zigbee end device
        EndDevice = 0x02,
        #[fallback = true]
        Invalid(u8),
    }
}

pub const NWK_COORDINATOR_ADDRESS: u16 = 0x0000;

/// Network address of a device that is not joined (3.6.1.10.4).
pub const NWK_UNASSIGNED_ADDRESS: u16 = 0xffff;

/// Broadcast to all devices (3.6.5, Table 3-68).
pub const NWK_BROADCAST_ALL: u16 = 0xffff;

/// Lowest reserved broadcast address; anything above is a broadcast (Table
/// 3-68).
pub const NWK_BROADCAST_ADDRESS_MIN: u16 = 0xfff8;

/// Neighbor table relationship values (Table 3-63).
pub mod relationship {
    /// The neighbor is the parent of this device.
    pub const PARENT: u8 = 0x00;
    /// The neighbor is a child of this device.
    pub const CHILD: u8 = 0x01;
    /// The neighbor is a sibling.
    pub const SIBLING: u8 = 0x02;
    /// No relationship.
    pub const NONE: u8 = 0x03;
    /// Previous child (relationship ended).
    pub const PREVIOUS_CHILD: u8 = 0x04;
    /// Unauthenticated child.
    pub const UNAUTHENTICATED_CHILD: u8 = 0x05;
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
// active plus alternate network key (4.6.3.4.2)
const MAX_SECURITY_KEYS: usize = 2;

/// Maximum acceptable link cost for parent selection (3.6.1.4.1.1).
pub const MAX_PARENT_LINK_COST: u8 = 3;

/// Computes the link cost (1-7, lower is better) from an LQI value
/// (3.6.3.1).
///
/// The LQI-to-cost mapping is implementation-defined; this uses a threshold
/// table suitable for IEEE 802.15.4 2.4 GHz PHY.
pub fn link_cost_from_lqi(lqi: u8) -> u8 {
    match lqi {
        // excellent link (>= -55 dBm)
        128..=255 => 1,
        // good link (>= -65 dBm)
        77..=127 => 2,
        // acceptable (>= -75 dBm)
        26..=76 => 3,
        // marginal
        11..=25 => 5,
        // poor
        1..=10 => 6,
        // very poor
        0 => 7,
    }
}

construct_ib! {
    /// Network Information Base.
    ///
    /// See Section 3.5.2.
    #[ids = NibId]
    #[fields = NibFields]
    pub struct Nib {
        /// Sequence number
        sequence_number / update_sequence_number: u8, // random value, read only
        #[storage_key = 9]
        passive_ack_timeout / update_passive_ack_timeout: u32, // stack profile
        #[storage_key = 10]
        max_broadcast_retries / update_max_broadcast_retries: u8 = 0x03,
        #[storage_key = 11]
        max_children / update_max_children: u8, // stack profile
        #[storage_key = 12]
        max_depth / update_max_depth: u8, // stack profile, read only
        #[storage_key = 13]
        max_routers / update_max_routers: u8, // stack profile
        #[storage_key = 8]
        neighbor_table / update_neighbor_table: StorageVec<NwkNeighbor, MAX_NEIGBOUR_TABLE>,
        #[storage_key = 14]
        network_broadcast_delivery_time / update_network_broadcast_delivery_time: u32, // stack profile
        #[storage_key = 15]
        report_constant_cost / update_report_constant_cost: u8 = 0x00, // 0x00 - 0x01
        route_table / update_route_table: StorageVec<NwkRoute, MAX_ROUTE_TABLE>,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 16]
        sym_link / update_sym_link: bool = false, // bool
        #[storage_key = 7]
        capability_information / update_capability_information: CapabilityInformation = CapabilityInformation(0x00), // read only
        #[storage_key = 17]
        addr_alloc / update_addr_alloc: u8 = 0x0, // 0x00 - 0x02
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 18]
        use_tree_routing / update_use_tree_routing: bool = true,
        #[storage_key = 19]
        manager_addr / update_manager_addr: u16 = 0x0000, // <= 0xfff7
        #[storage_key = 20]
        max_source_route / update_max_source_route: u8 = 0x0c,
        #[storage_key = 4]
        update_id / update_update_id: u8 = 0x00,
        #[storage_key = 21]
        transaction_persistence_time / update_transaction_persistence_time: u16 = 0x01f4,
        #[storage_key = 1]
        network_address / update_network_address: u16 = 0xffff, //  <= 0xfff7
        #[storage_key = 22]
        stack_profile / update_stack_profile: u8, // <= 0x0f
        broadcast_transaction_table / update_broadcast_transaction_table: StorageVec<TransactionRecord, MAX_BROADCAST_TRANSACTION_TABLE>,
        #[storage_key = 23]
        group_idtable / update_group_idtable: StorageVec<u16, MAX_GROUP_ID_TABLE>,
        #[storage_key = 3]
        extended_panid / update_extended_panid: u64 = 0x0000_0000_0000_0000, // <= 0xffff_ffff_ffff_fffe
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 24]
        use_multicast / update_use_multicast: bool = true,
        route_record_table / update_route_record_table: StorageVec<RouteRecord, MAX_ROUTE_RECORD_TABLE>,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 25]
        is_concentrator / update_is_concentrator: bool = false,
        #[storage_key = 26]
        concentrator_radius / update_concentrator_radius: u8 = 0x00,
        #[storage_key = 27]
        concentrator_discovery_time / update_concentrator_discovery_time: u8 = 0x00,
        // nib security attributes
        #[storage_key = 28]
        security_level / update_security_level: SecurityLevel = SecurityLevel::EncMic32,
        #[storage_key = 6]
        security_material_set / update_security_material_set: StorageVec<NetworkSecurityMaterialDescriptor, MAX_SECURITY_KEYS>,
        #[storage_key = 5]
        active_key_seq_number / update_active_key_seq_number: u8 = 0x00,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 29]
        all_fresh / update_all_fresh: bool = true,

        #[storage_key = 30]
        link_status_period / update_link_status_period: u8 = 0x0f,
        #[storage_key = 31]
        router_age_limit / update_router_age_limit: u8 = 0x03,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 32]
        unique_addr / update_unique_addr: bool = true,
        #[storage_key = 33]
        address_map / update_address_map: StorageVec<AddressMap, MAX_NWK_ADDRESS_MAP>,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 34]
        time_stamp / update_time_stamp: bool = false,
        #[storage_key = 2]
        panid / update_panid: u16 = 0xffff,
        tx_total / update_tx_total: u16 = 0x0000,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 35]
        leave_request_allowed / update_leave_request_allowed: bool = true,
        #[storage_key = 36]
        parent_information / update_parent_information: u8 = 0x00,
        #[storage_key = 37]
        end_device_timeout_default / update_end_device_timeout_default: u8 = 0x08,
        // negotiated timeout enumeration (3.6.10.2); 0xff = not negotiated
        #[storage_key = 39]
        end_device_timeout / update_end_device_timeout: u8 = 0xff,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 38]
        leave_request_without_rejoin_allowed / update_leave_request_without_rejoin_allowed: bool = true,
        ieee_address / update_ieee_address: IeeeAddress, // read only
        // TODO: mac_interface_table (StorageVec<MacInterface, MAX_MAC_INTERFACE_TABLE>) not yet implemented
    }
}

impl_byte! {
    #[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
    pub struct CapabilityInformation(pub u8);
}

impl CapabilityInformation {
    /// Bit 0 — Alternate PAN coordinator (always 0 in Zigbee).
    pub fn alternate_pan_coordinator(&self) -> bool {
        self.0 & (1 << 0) != 0
    }

    /// Bit 1 — Device type: 1 = router (FFD), 0 = end device (RFD).
    pub fn device_type(&self) -> bool {
        self.0 & (1 << 1) != 0
    }

    /// Bit 2 — Power source: 1 = mains-powered, 0 = other.
    pub fn power_source(&self) -> bool {
        self.0 & (1 << 2) != 0
    }

    /// Bit 3 — Receiver on when idle.
    pub fn receiver_on_when_idle(&self) -> bool {
        self.0 & (1 << 3) != 0
    }

    /// Bit 6 — Security capability (always 0 in Zigbee).
    pub fn security_capability(&self) -> bool {
        self.0 & (1 << 6) != 0
    }

    /// Bit 7 — Allocate address: 1 = coordinator should allocate a short
    /// address.
    pub fn allocate_address(&self) -> bool {
        self.0 & (1 << 7) != 0
    }
}

impl_byte! {
    #[derive(Debug, Clone)]
    pub struct NwkNeighbor {
        /// IEEE address of the neighbor, learned from received frames that
        /// carry it; `IeeeAddress(0)` while unknown.
        pub extended_address: IeeeAddress,
        pub network_address: ShortAddress,
        pub device_type: DeviceType,
        #[ctx = ()]
        pub rx_on_when_idle: bool,
        pub end_device_configuration: u16,
        pub relationship: u8,
        pub transmit_failure: u8,
        pub lqi: u8,
        pub outgoing_cost: u8,
        pub age: u8,
        #[ctx = ()]
        pub keepalive_received: bool,
        // mac_interface_index omitted: only one mac interface is supported

        // table 3-64: optional discovery-time fields, cleared after joining

        /// Extended PAN identifier of the network the neighbor belongs to.
        pub extended_pan_id: IeeeAddress,
        /// The logical channel on which the neighbor is operating.
        pub logical_channel: u8,
        /// The tree depth of the neighbor device.
        pub depth: u8,
        /// Whether the neighbor is accepting join requests.
        #[ctx = ()]
        pub permit_joining: bool,
        /// 0x00 = not a potential parent, 0x01 = potential parent.
        pub potential_parent: u8,
        /// Whether the neighbor can accept router-capable children.
        #[ctx = ()]
        pub router_capacity: bool,
        /// Whether the neighbor can accept end-device children.
        #[ctx = ()]
        pub end_device_capacity: bool,
        /// The nwkUpdateId from the beacon payload.
        pub update_id: u8,
        /// The 16-bit PAN identifier.
        pub pan_id: u16,
    }
}

/// See Table 3-67.
#[derive(Debug, Clone)]
#[repr(u8)]
pub(crate) enum RouteStatus {
    Active,
    DiscoveryUnderway,
    DiscoveryFailed,
    Inactive,
    ValidationUnderway,
    Reserved,
}

impl_byte! {
    /// See Table 3-66.
    #[derive(Debug, Clone)]
    pub struct NwkRoute {
        pub destination_address: ShortAddress,
        pub next_hop_address: ShortAddress,
        status: u8,
    }
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

impl_byte! {
    /// See Table 3-70.
    #[derive(Debug, Clone)]
    pub struct TransactionRecord {
        pub source_address: ShortAddress,
        pub sequence_number: u8,
        pub expiration_time: u8,
    }
}

impl_byte! {
    /// See Table 3-59.
    #[derive(Debug, Clone)]
    pub struct RouteRecord {
        pub network_address: ShortAddress,
        pub relay_count: u16,
        pub path: StorageVec<ShortAddress, 16>,
    }
}

impl_byte! {
    #[derive(Debug, Clone)]
    pub struct AddressMap {
        pub ieee_address: IeeeAddress,
        pub network_address: ShortAddress,
    }
}

/// See Table 3-61.
#[derive(Debug, Clone)]
pub(crate) struct MacInterface {}

impl_byte! {
    /// See Table 4-3.
    #[derive(Debug, Clone)]
    pub struct NetworkSecurityMaterialDescriptor {
        pub key_seq_number: u8,
        pub outgoing_frame_counter: u32,
        pub incoming_frame_counter_set: StorageVec<IncomingFrameCounterDescriptor, MAX_NEIGBOUR_TABLE>,
        pub key: ByteArray<16>,
        pub network_key_type: u8,
    }
}

impl_byte! {
    /// See Table 4-4.
    #[derive(Debug, Clone)]
    pub struct IncomingFrameCounterDescriptor {
        pub sender_address: IeeeAddress,
        pub incoming_frame_counter: u32,
    }
}

/// Flash persistence of the NIB.
#[cfg(feature = "storage")]
pub(crate) mod storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nib_set_vec() {
        let nib = Nib::new();

        let mut set = StorageVec::<NetworkSecurityMaterialDescriptor, MAX_SECURITY_KEYS>::new();
        set.push(NetworkSecurityMaterialDescriptor {
            key_seq_number: 0,
            outgoing_frame_counter: 0,
            incoming_frame_counter_set: StorageVec(Vec::new()),
            key: ByteArray([0u8; 16]),
            network_key_type: 0,
        })
        .unwrap();
        nib.update_security_material_set(|value| *value = set);
        assert_eq!(nib.security_material_set().len(), 1);
    }

    #[test]
    fn nib_default() {
        // local instance: the global singleton is shared with (and mutated
        // by) other tests running in parallel
        let nib = Nib::new();

        assert_eq!(*nib.max_broadcast_retries(), 0x03);
        assert_eq!(*nib.report_constant_cost(), 0x00);
        assert!(!*nib.sym_link());
        assert_eq!(*nib.capability_information(), CapabilityInformation(0x00));
        assert_eq!(*nib.addr_alloc(), 0x0);
        assert!(*nib.use_tree_routing());
        assert_eq!(*nib.manager_addr(), 0x0000);
        assert_eq!(*nib.max_source_route(), 0x0c);
        assert_eq!(*nib.update_id(), 0x00);
        assert_eq!(*nib.transaction_persistence_time(), 0x01f4);
        assert_eq!(*nib.network_address(), 0xffff);
        assert_eq!(*nib.extended_panid(), 0x0000_0000_0000_0000);
        assert!(*nib.use_multicast());
        assert!(!*nib.is_concentrator());
        assert_eq!(*nib.concentrator_radius(), 0x00);
        assert_eq!(*nib.concentrator_discovery_time(), 0x00);
        assert_eq!(*nib.link_status_period(), 0x0f);
        assert_eq!(*nib.router_age_limit(), 0x03);
        assert!(*nib.unique_addr());
        assert!(!*nib.time_stamp());
        assert_eq!(*nib.panid(), 0xffff);
        assert_eq!(*nib.tx_total(), 0x0000);
        assert!(*nib.leave_request_allowed());
        assert_eq!(*nib.parent_information(), 0x00);
        assert_eq!(*nib.end_device_timeout_default(), 0x08);
        assert!(*nib.leave_request_without_rejoin_allowed());
    }

    #[test]
    fn storage_vec() {
        let mut vec = StorageVec::<u8, 3>::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        let mut buf = [0u8; 5];
        vec.try_write(&mut buf, byte::LE).unwrap();
        assert_eq!(buf, [0x03, 0x00, 0x01, 0x02, 0x03]);

        let (vec2, _) = StorageVec::<u8, 3>::try_read(&buf, byte::LE).unwrap();
        assert_eq!(vec2.len(), 3);
        assert_eq!(vec2[0], 1);
        assert_eq!(vec2[1], 2);
        assert_eq!(vec2[2], 3);
    }
}
