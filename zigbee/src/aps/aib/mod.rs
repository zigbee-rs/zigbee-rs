use zigbee_macros::construct_ib;
use zigbee_macros::impl_byte;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::StorageVec;

const MAX_APS_BINDING_TABLE: usize = 2; // TODO
const MAX_APS_CHANNEL_MASK_LIST: usize = 2; // TODO
const MAX_APS_GROUP_TABLE: usize = 2; // TODO
const MAX_APS_MAX_WINDOW_SIZE: usize = 2; // TODO
const MAX_APS_DEVICE_KEY_PAIR_SET: usize = 2; // TODO

construct_ib! {
    /// 2.2.7.2 - AIB (APS Information Base Attributes)
    pub struct Aib {
        //apsBindingTable
        #[storage_key = 4]
        binding_table: StorageVec<ApsBinding, MAX_APS_BINDING_TABLE>,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 6]
        designated_coordinator: bool = false,
        #[storage_key = 7]
        channel_mask_list: StorageVec<IeeeAddress, MAX_APS_CHANNEL_MASK_LIST>,
        #[storage_key = 3]
        use_extended_pan_id: IeeeAddress,
        #[storage_key = 5]
        group_table: StorageVec<ApsGroup, MAX_APS_GROUP_TABLE>,
        #[storage_key = 8]
        non_member_radius: u8 = 0x02,
        #[ctx = ()]
        #[ctx_write = ()]
        #[storage_key = 9]
        use_insecure_join: bool = false,
        #[storage_key = 10]
        interframe_delay: u8,
        last_channel_energy: u8 = 0x00,
        last_channel_failure_rate: u8 = 0x00,
        channel_timer: u8 = 0x00,
        #[storage_key = 11]
        max_window_size: StorageVec<ApsWindowSize, MAX_APS_MAX_WINDOW_SIZE>,
        parent_announce_timer: u8 = 0x00,
        // security attributes
        #[storage_key = 2]
        device_key_pair_set: StorageVec<DeviceKeyPairDescriptor, MAX_APS_DEVICE_KEY_PAIR_SET>,
        #[storage_key = 1]
        trust_center_address: IeeeAddress = IeeeAddress(0xffff_ffff_ffff_ffff),
        #[storage_key = 12]
        security_timeout_period: u16 = 0x00,
        //trust_center_policues: u8, // not implemented
    }
}

// TODO
impl_byte! {
    #[derive(Debug, Clone)]
    pub struct ApsBinding(u8);
}

// TODO
impl_byte! {
    #[derive(Debug, Clone)]
    pub struct ApsGroup(u8);
}

// TODO
impl_byte! {
    #[derive(Debug, Clone)]
    pub struct ApsWindowSize(u8);
}

impl_byte! {
    #[derive(Debug, Clone)]
    pub struct DeviceKeyPairDescriptor {
        pub device_address: IeeeAddress,
        pub key_attributes: KeyAttribute,
        pub link_key: ByteArray<16>,
        pub outgoing_frame_counter: u32,
        pub incoming_frame_counter: u32,
        pub link_key_type: LinkKeyType,
    }
}

impl_byte! {
    #[tag(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum KeyAttribute {
        ProvisionalKey = 0x00,
        UnverifiedKey = 0x01,
        VerifiedKey = 0x02,
        #[fallback = true]
        Reserved(u8),
    }
}

impl_byte! {
    #[tag(u8)]
    #[derive(Debug, Clone)]
    pub enum LinkKeyType {
        UniqueLinkKey = 0x00,
        GlobalLinkKey = 0x01,
        #[fallback = true]
        Reserved(u8),
    }
}

/// Flash persistence of the AIB.
#[cfg(feature = "storage")]
pub(crate) mod storage;
