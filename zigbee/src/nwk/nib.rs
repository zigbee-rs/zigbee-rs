//! NWK information base
//!
//! See Section 3.5.
use core::mem;

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use embedded_storage::ReadStorage;
use embedded_storage::Storage;
use heapless::FnvIndexMap;
use heapless::Vec;
use spin::Mutex;

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

macro_rules! construct_nib {
    (
        $(
            $(#[doc = $doc:literal])*
            $(#[ctx = $ctx_hdr:expr])?
            $(#[ctx_write = $ctx_write:expr])?
            $field:ident: $field_ty:path $(= $default:expr)?,
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

        pub const NIB_BUFFER_SIZE: usize = nib_buffer_size();

        const fn nib_buffer_size() -> usize {
            let mut size = 0usize;
            let mut i = 0;
            while i < NIB_ID_SIZE_LUT.len() {
                size += NIB_ID_SIZE_LUT[i];
                i += 1;
            }
            size
        }

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

        /// Network Information Base.
        ///
        /// See Section 3.5.2.
        pub struct Nib<C> {
            storage: Mutex<C>,
        }

        #[allow(clippy::cast_possible_truncation)]
        impl<C: Storage> Nib<C> {
            pub fn new(storage: C) -> Self {
                Self { storage: Mutex::new(storage) }
            }

            pub fn init(&self) {
                $(
                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_write;
                    )?
                    $(
                        let mut buf = [0u8; NibId::$field.size()];
                        let value: $field_ty = $default;
                        buf.write_with(&mut 0, value, cx).expect("init");
                        let _ = self.storage.lock().write(NibId::$field.offset() as u32, &buf);
                    )?
                )+
            }

            $(
                $(#[doc = $doc])*
                pub fn $field(&self) -> $field_ty {
                    const SIZE: usize = NibId::$field.size();
                    let mut buf = [0u8; SIZE];
                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_hdr;
                    )?

                    let _ = self.storage.lock().read(NibId::$field.offset() as u32, &mut buf);
                    buf.read_with(&mut 0, cx).unwrap()
                }

                pub fn ${ concat(set_, $field) }(&self, value: $field_ty) {
                    const SIZE: usize = NibId::$field.size();
                    let mut buf = [0u8; SIZE];

                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_write;
                    )?
                    buf.write_with(&mut 0, value, cx).unwrap();

                    let _ = self.storage.lock().write(NibId::$field.offset() as u32, &buf);
                }
            )+
        }
    };
}

construct_nib! {
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
    security_level: u8,
    security_material_set: u8,
    active_key_seq_number: u8,
    all_fresh: u8,
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

impl_byte! {
    #[derive(Debug, Default)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::storage::InMemoryStorage;

    #[test]
    fn nib_init() {
        let nib = Nib::new(InMemoryStorage::<NIB_BUFFER_SIZE>::default());
        nib.init();

        nib.set_max_broadcast_retries(0x03);
        assert_eq!(nib.transaction_persistence_time(), 0x01f4);
    }
}
