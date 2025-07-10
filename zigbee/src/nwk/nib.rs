//! NWK information base
//!
//! See Section 3.5.
#![allow(dead_code)]

use core::mem;

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use heapless::FnvIndexMap;
use heapless::Vec;

use crate::impl_byte;
use crate::internal::types::IeeeAddress;
use crate::internal::types::ShortAddress;
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

#[derive(Debug, Default)]
pub(crate) struct CapabilityInformation(u8);

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

macro_rules! construct_nib {
    (
        $(
            $(#[ctx = $ctx_hdr:expr])?
            $(#[ctx_write = $ctx_write:expr])?
            $field:ident: $field_ty:path $(= $default:literal)?,
        )+
    ) => {
        #[repr(usize)]
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, PartialEq)]
        enum NibId {
            $($field),+
        }

        // might not be the exact size of the field
        // because encoding (produced by byte::TryWrite)
        // might be different than struct alignment
        // but `size_of` gives us an upper bound
        const NIB_ID_SIZE_LUT: &[usize] = &[
            $(
                size_of::<$field_ty>()
            ),+
        ];

        impl NibId {
            const fn size(&self) -> usize {
                NIB_ID_SIZE_LUT[*self as usize]
            }

            const fn offset(&self) -> usize {
                let mut i = 0usize;
                let mut offset = 0usize;
                while i != *self as usize {
                    offset += NIB_ID_SIZE_LUT[i];
                    i += 1;
                }
                offset
            }
        }


        pub struct NibV2 {
            buf: [u8; Self::BUF_SIZE],
        }

        impl NibV2 {
            const BUF_SIZE: usize = 2048;

            pub fn init() -> Self {
                let buf = [0u8; Self::BUF_SIZE];
                let mut nib = Self { buf };
                $(
                    $(
                        nib.${ concat(set_, $field) }($default);
                    )?
                )+

                nib
            }

            $(
                pub fn $field(&self) -> $field_ty {
                    let mut offset = NibId::$field.offset();
                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_hdr;
                    )?
                    self.buf.read_with(&mut offset, cx).unwrap()
                }

                pub fn ${ concat(set_, $field) }(&mut self, value: $field_ty) {
                    let mut offset = NibId::$field.offset();
                    let size = NibId::$field.size();
                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_write;
                    )?
                    self.buf.write_with(&mut offset, value, cx).unwrap();
                }
            )+
        }
    };
}

construct_nib! {
    passive_ack_timeout: u32 = 0x0000_0000,
    sequence_number: u8 = 0x0,
    max_broadcast_retries: u8 = 0x03,
    max_children: u8 = 0x00,
    max_depth: u8 = 0x00,
    max_routers: u8 = 0x00,
    neighbor_table: StorageVec<NwkNeighbor, MAX_NEIGBOUR_TABLE>,
    network_broadcast_delivery_time: u32 = 0x0000_0000,
    report_constant_cost: u8 = 0x00,
    route_table: StorageVec<NwkRoute, MAX_ROUTE_TABLE>,
    sym_link: u8 = 0x00,
    capability_information: u8 = 0x0,
    addr_alloc: u8 = 0x0,
    use_tree_routing: u8 = 0x01,
    manager_addr: u16 = 0x0000,
    max_source_route: u8 = 0x0c,
    update_id: u8 = 0x00,
    transaction_persistence_time: u16 = 0x01f4,
    network_address: u16 = 0xffff,
    stack_profile: u8 = 0x00,
    broadcast_transaction_table: StorageVec<TransactionRecord, MAX_BROADCAST_TRANSACTION_TABLE>,
    group_idtable: StorageVec<u16, MAX_GROUP_ID_TABLE>,
    extended_panid: u64,
    use_multicast: u8 = 0x01,
    route_record_table: StorageVec<RouteRecord, MAX_ROUTE_RECORD_TABLE>,
    is_concentrator: u8 = 0x00,
    concentrator_radius: u8 = 0x00,
    concentrator_discovery_time: u8 = 0x00,
    security_level: u8 = 0x00,
    security_material_set: u8 = 0x00,
    active_key_seq_number: u8 = 0x00,
    all_fresh: u8 = 0x00,
    link_status_period: u8 = 0x0f,
    router_age_limit: u8 = 0x03,
    unique_addr: u8 = 0x01,
    address_map: StorageVec<AddressMap, MAX_NWK_ADDRESS_MAP>,
    time_stamp: u8 = 0x00,
    panid: u16 = 0x0000,
    tx_total: u16 = 0x0000,
    leave_request_allowed: u8 = 0x01,
    parent_information: u8 = 0x00,
    end_device_timeout_default: u8 = 0x08,
    leave_request_without_rejoin_allowed: u8 = 0x01,
    ieee_address: IeeeAddress,
    // mac_interface_table: StorageVec<MacInterface, MAX_MAC_INTERFACE_TABLE>,
}

#[derive(Debug)]
pub struct StorageVec<T, const N: usize>(pub Vec<T, N>);

impl<'a, const N: usize, C, T> TryRead<'a, C> for StorageVec<T, N>
where
    C: Default + Copy + Clone,
    T: TryRead<'a, C>,
{
    fn try_read(bytes: &'a [u8], ctx: C) -> Result<(Self, usize), byte::Error> {
        let offset = &mut 0;
        // first 2 bytes is the length, should be enough
        let len: u16 = bytes.read_with(offset, byte::LE)?;

        let mut data: Vec<T, N> = Vec::new();
        for _i in 0..len {
            let entry: T = bytes.read_with(offset, ctx)?;
            let _ = data.push(entry);
        }
        Ok((Self(data), *offset))
    }
}

impl<const N: usize, C, T> TryWrite<C> for StorageVec<T, N>
where
    C: Default + Copy + Clone,
    T: TryWrite<C>,
{
    #[allow(clippy::cast_possible_truncation)]
    fn try_write(self, bytes: &mut [u8], ctx: C) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        // first 2 bytes is the length
        bytes.write_with(&mut 0, self.0.len() as u16, byte::LE)?;
        for entry in self.0 {
            bytes.write_with(&mut 0, entry, ctx)?;
        }
        Ok(*offset)
    }
}
