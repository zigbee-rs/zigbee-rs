use ieee802154::mac::Address;
use ieee802154::mac::beacon::SuperframeSpecification;
use thiserror::Error;
use zigbee_macros::impl_byte;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

pub const MAX_IEEE802154_CHANNELS: usize = 27;

pub const A_BASE_SLOT_DURATION: u32 = 60;
pub const A_NUM_SUPER_FRAME_SLOTS: u32 = 16;
pub const A_BASE_SUPER_FRAME_DURATION: u32 = A_BASE_SLOT_DURATION * A_NUM_SUPER_FRAME_SLOTS;

pub trait Mlme {
    async fn scan_network(
        &mut self,
        ty: ScanType,
        channels: impl Iterator<Item = u8>,
        duration: u8,
    ) -> Result<ScanResult, MacError>;
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    Ed,
    Active,
    Passive,
    Orphan,
}

#[derive(Debug, Error)]
pub enum MacError {
    #[error("no beacons received")]
    NoBeacon,
    #[error("invalid scan parameters")]
    InvalidScanParams,
    #[error("read error")]
    ReadError(byte::Error),
    #[error("radio error")]
    RadioError,
}

#[derive(Debug)]
pub struct ScanResult {
    pub scan_type: ScanType,
    #[cfg(feature = "alloc")]
    pub pan_descriptor: alloc::vec::Vec<PanDescriptor>,
    #[cfg(not(feature = "alloc"))]
    pub pan_descriptor: heapless::Vec<PanDescriptor, MAX_IEEE802154_CHANNELS>,
}

#[non_exhaustive]
#[derive(Debug)]
pub struct PanDescriptor {
    pub channel: u8,
    pub coord_addr_mode: u8,
    pub coord_pan_id: ShortAddress,
    pub coord_address: Address,
    pub superframe_spec: SuperframeSpecification,
    //pub gts_permit: bool,
    pub link_quality: u8,
    //pub timestamp: u32,
    pub security_use: bool,
    //pub ACL_Entry: u8,
    //pub security_failure: bool
    pub zigbee_beacon: ZigbeeBeacon,
}

impl_byte! {
    #[derive(Debug)]
    pub struct ZigbeeBeacon {
        pub protocol_id: u8,
        pub stack_profile: StackProfile,
        pub extended_pan_id: IeeeAddress,
        pub tx_offset: ByteArray<3>,
        pub update_id: u8,
    }
}

impl_byte! {
    /// Stack Profile field
    ///
    /// See ZigBee specification Annex D for bit field layout.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct StackProfile(pub u16);
}

impl core::fmt::Debug for StackProfile {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StackProfile")
            .field("stack_profile", &self.stack_profile())
            .field("protocol_version", &self.protocol_version())
            .field("router_capacity", &self.router_capacity())
            .field("device_depth", &self.device_depth())
            .field("end_device_capacity", &self.end_device_capacity())
            .finish()
    }
}

impl StackProfile {
    /// Stack profile value
    pub fn stack_profile(&self) -> u8 {
        ((self.0 & mask::STACK_PROFILE) >> offset::STACK_PROFILE) as u8
    }

    /// Sets the stack profile value
    #[must_use]
    pub fn set_stack_profile(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::STACK_PROFILE)
            | ((value as u16 & (mask::STACK_PROFILE >> offset::STACK_PROFILE))
                << offset::STACK_PROFILE);
        self
    }

    /// Protocol version
    pub fn protocol_version(&self) -> u8 {
        ((self.0 & mask::PROTOCOL_VERSION) >> offset::PROTOCOL_VERSION) as u8
    }

    /// Sets the protocol version
    #[must_use]
    pub fn set_protocol_version(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::PROTOCOL_VERSION)
            | ((value as u16 & (mask::PROTOCOL_VERSION >> offset::PROTOCOL_VERSION))
                << offset::PROTOCOL_VERSION);
        self
    }

    /// Route capacity flag
    pub fn router_capacity(&self) -> bool {
        ((self.0 & mask::ROUTER_CAPACITY) >> offset::ROUTER_CAPACITY) != 0
    }

    /// Sets the route capacity flag
    #[must_use]
    pub fn set_router_capacity(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::ROUTER_CAPACITY) | (u16::from(value) << offset::ROUTER_CAPACITY);
        self
    }

    /// Device depth
    pub fn device_depth(&self) -> u8 {
        ((self.0 & mask::DEVICE_DEPTH) >> offset::DEVICE_DEPTH) as u8
    }

    /// Sets the device depth
    #[must_use]
    pub fn set_device_depth(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::DEVICE_DEPTH)
            | ((value as u16 & (mask::DEVICE_DEPTH >> offset::DEVICE_DEPTH))
                << offset::DEVICE_DEPTH);
        self
    }

    /// End device capacity flag
    pub fn end_device_capacity(&self) -> bool {
        ((self.0 & mask::END_DEVICE_CAPACITY) >> offset::END_DEVICE_CAPACITY) != 0
    }

    /// Sets the end device capacity flag
    #[must_use]
    pub fn set_end_device_capacity(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::END_DEVICE_CAPACITY)
            | (u16::from(value) << offset::END_DEVICE_CAPACITY);
        self
    }
}

mod offset {
    pub const STACK_PROFILE: u16 = 0;
    pub const PROTOCOL_VERSION: u16 = 4;
    pub const ROUTER_CAPACITY: u16 = 10;
    pub const DEVICE_DEPTH: u16 = 11;
    pub const END_DEVICE_CAPACITY: u16 = 15;
}

mod mask {
    // stack_profile: bits 0-4 (5 bits)
    pub const STACK_PROFILE: u16 = 0x1F;
    // protocol_version: bits 4-7 (4 bits)
    pub const PROTOCOL_VERSION: u16 = 0xF0;
    // route_capacity: bit 10 (1 bit)
    pub const ROUTER_CAPACITY: u16 = 0x400;
    // device_depth: bits 11-14 (4 bits)
    pub const DEVICE_DEPTH: u16 = 0x7800;
    // end_device_capacity: bit 15 (1 bit)
    pub const END_DEVICE_CAPACITY: u16 = 0x8000;
}
