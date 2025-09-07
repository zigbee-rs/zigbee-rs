//! NWK information base
//!
//! See Section 3.5.
use core::mem;
use core::ops::Deref;
use core::ops::DerefMut;

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use embedded_storage::ReadStorage;
use embedded_storage::Storage;
use heapless::FnvIndexMap;
use heapless::Vec;
use spin::Mutex;

use crate::construct_ib;
use crate::impl_byte;
use crate::internal::storage::InMemoryStorage;
use crate::internal::types::ByteArray;
use crate::internal::types::IeeeAddress;
use crate::internal::types::ShortAddress;
use crate::internal::types::StorageVec;
use crate::security::frame::SecurityLevel;

impl_byte! {
    #[tag(u8)]
    /// Zigbee device type.
    #[derive(Debug)]
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
const MAX_SECURITY_KEYS: usize = 1;

construct_ib! {
    /// Network Information Base.
    ///
    /// See Section 3.5.2.
    pub struct Nib {
        /// Sequence number
        sequence_number: u8, // random value, read only
        passive_ack_timeout: u32, // stack profile
        max_broadcast_retries: u8 = 0x03,
        max_children: u8, // stack profile
        max_depth: u8, // stack profile, read only
        max_routers: u8, // stack profile
        neighbor_table: StorageVec<NwkNeighbor, MAX_NEIGBOUR_TABLE>,
        network_broadcast_delivery_time: u32, // stack profile
        report_constant_cost: u8 = 0x00, // 0x00 - 0x01
        route_table: StorageVec<NwkRoute, MAX_ROUTE_TABLE>,
        #[ctx = ()]
        #[ctx_write = ()]
        sym_link: bool = false, // bool
        capability_information: CapabilityInformation = CapabilityInformation(0x00), // read only
        addr_alloc: u8 = 0x0, // 0x00 - 0x02
        #[ctx = ()]
        #[ctx_write = ()]
        use_tree_routing: bool = true,
        manager_addr: u16 = 0x0000, // <= 0xfff7
        max_source_route: u8 = 0x0c,
        update_id: u8 = 0x00,
        transaction_persistence_time: u16 = 0x01f4,
        network_address: u16 = 0xffff, //  <= 0xfff7
        stack_profile: u8, // <= 0x0f
        broadcast_transaction_table: StorageVec<TransactionRecord, MAX_BROADCAST_TRANSACTION_TABLE>,
        group_idtable: StorageVec<u16, MAX_GROUP_ID_TABLE>,
        extended_panid: u64 = 0x0000_0000_0000_0000, // <= 0xffff_ffff_ffff_fffe
        #[ctx = ()]
        #[ctx_write = ()]
        use_multicast: bool = true,
        route_record_table: StorageVec<RouteRecord, MAX_ROUTE_RECORD_TABLE>,
        #[ctx = ()]
        #[ctx_write = ()]
        is_concentrator: bool = false,
        concentrator_radius: u8 = 0x00,
        concentrator_discovery_time: u8 = 0x00,
        // nib security attributes
        security_level: SecurityLevel = SecurityLevel::EncMic32,
        security_material_set: StorageVec<NetworkSecurityMaterialDescriptor, MAX_SECURITY_KEYS>,
        active_key_seq_number: u8 = 0x00,
        #[ctx = ()]
        #[ctx_write = ()]
        all_fresh: bool = true,

        link_status_period: u8 = 0x0f,
        router_age_limit: u8 = 0x03,
        #[ctx = ()]
        #[ctx_write = ()]
        unique_addr: bool = true,
        address_map: StorageVec<AddressMap, MAX_NWK_ADDRESS_MAP>,
        #[ctx = ()]
        #[ctx_write = ()]
        time_stamp: bool = false,
        panid: u16 = 0xffff,
        tx_total: u16 = 0x0000,
        #[ctx = ()]
        #[ctx_write = ()]
        leave_request_allowed: bool = true,
        parent_information: u8 = 0x00,
        end_device_timeout_default: u8 = 0x08,
        #[ctx = ()]
        #[ctx_write = ()]
        leave_request_without_rejoin_allowed: bool = true,
        ieee_address: IeeeAddress, // read only
        // mac_interface_table: StorageVec<MacInterface, MAX_MAC_INTERFACE_TABLE>,
    }
}

impl_byte! {
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct CapabilityInformation(u8);
}

impl_byte! {
    #[derive(Debug)]
    pub struct NwkNeighbor {
        extended_address: IeeeAddress,
        network_address: ShortAddress,
        device_type: DeviceType,
        #[ctx = ()]
        rx_on_when_idle: bool,
        end_device_configuration: u16,
        timeout_counter: u32,
        device_timeout: u32,
        relationship: u8,
        transmit_failure: u8,
        lqi: u8,
        outgoing_cost: u8,
        age: u8,
        incoming_beacon_timestamp: u8,
        beacon_transmission_time: u8,
        #[ctx = ()]
        keepalive_received: bool,
        mac_interface_index: u8,
        mac_unicast_bytes_transmitted: u32,
        mac_unicast_bytes_received: u32,
    }
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

impl_byte! {
    /// See Table 3-66.
    #[derive(Debug)]
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
    #[derive(Debug)]
    pub struct TransactionRecord {
        pub source_address: ShortAddress,
        pub sequence_number: u8,
        pub expiration_time: u8,
    }
}

impl_byte! {
    /// See Table 3-59.
    #[derive(Debug)]
    pub struct RouteRecord {
        pub network_address: ShortAddress,
        pub relay_count: u16,
        pub path: StorageVec<ShortAddress, 16>,
    }
}

impl_byte! {
    pub struct AddressMap {
        pub ieee_address: IeeeAddress,
        pub network_address: ShortAddress,
    }
}

/// See Table 3-61.
#[derive(Debug)]
pub(crate) struct MacInterface {}

impl_byte! {
    /// See Table 4-3.
    #[derive(Debug)]
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
    #[derive(Debug)]
    pub struct IncomingFrameCounterDescriptor {
        pub sender_address: IeeeAddress,
        pub incoming_frame_counter: u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nib_set_vec() {
        let nib = Nib::new(NibStorage::default());
        nib.init();

        let mut set = StorageVec::<NetworkSecurityMaterialDescriptor, 1>::new();
        set.push(NetworkSecurityMaterialDescriptor {
            key_seq_number: 0,
            outgoing_frame_counter: 0,
            incoming_frame_counter_set: StorageVec(Vec::new()),
            key: ByteArray([0u8; 16]),
            network_key_type: 0,
        })
        .unwrap();
        nib.set_security_material_set(set);
        assert_eq!(nib.security_material_set().len(), 1);
    }

    #[test]
    fn nib_default() {
        init(NibStorage::default());
        let nib = get_ref();

        assert_eq!(nib.max_broadcast_retries(), 0x03);
        assert_eq!(nib.report_constant_cost(), 0x00);
        assert!(!nib.sym_link());
        assert_eq!(nib.capability_information(), CapabilityInformation(0x00));
        assert_eq!(nib.addr_alloc(), 0x0);
        assert!(nib.use_tree_routing());
        assert_eq!(nib.manager_addr(), 0x0000);
        assert_eq!(nib.max_source_route(), 0x0c);
        assert_eq!(nib.update_id(), 0x00);
        assert_eq!(nib.transaction_persistence_time(), 0x01f4);
        assert_eq!(nib.network_address(), 0xffff);
        assert_eq!(nib.extended_panid(), 0x0000_0000_0000_0000);
        assert!(nib.use_multicast());
        assert!(!nib.is_concentrator());
        assert_eq!(nib.concentrator_radius(), 0x00);
        assert_eq!(nib.concentrator_discovery_time(), 0x00);
        assert_eq!(nib.link_status_period(), 0x0f);
        assert_eq!(nib.router_age_limit(), 0x03);
        assert!(nib.unique_addr());
        assert!(!nib.time_stamp());
        assert_eq!(nib.panid(), 0xffff);
        assert_eq!(nib.tx_total(), 0x0000);
        assert!(nib.leave_request_allowed());
        assert_eq!(nib.parent_information(), 0x00);
        assert_eq!(nib.end_device_timeout_default(), 0x08);
        assert!(nib.leave_request_without_rejoin_allowed());
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
