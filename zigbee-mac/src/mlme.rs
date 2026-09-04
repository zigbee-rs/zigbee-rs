use ieee802154::mac::Address;
use ieee802154::mac::PanId;
use ieee802154::mac::beacon::SuperframeSpecification;
use ieee802154::mac::command::AssociationStatus;
use ieee802154::mac::command::CapabilityInformation;
use thiserror::Error;
use zigbee_macros::impl_byte;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

pub const MAX_IEEE802154_CHANNELS: usize = 27;

pub const MAX_PAN_DESCRIPTOR_SIZE: usize = 27;

pub const A_BASE_SLOT_DURATION: u32 = 60;
pub const A_NUM_SUPER_FRAME_SLOTS: u32 = 16;
pub const A_BASE_SUPER_FRAME_DURATION: u32 = A_BASE_SLOT_DURATION * A_NUM_SUPER_FRAME_SLOTS;

/// The maximum time, in symbols, a device shall wait for a response command
/// to be available following a request command. See IEEE 802.15.4-2003 7.4.2.
/// aResponseWaitTime = 32 * aBaseSuperframeDuration = 32 * 960 = 30720 symbols.
pub const A_RESPONSE_WAIT_TIME: u32 = 32 * A_BASE_SUPER_FRAME_DURATION;

/// The maximum number of retries allowed after a transmission failure (i.e. a
/// missing acknowledgment). See IEEE 802.15.4-2003 7.4.2 (aMaxFrameRetries).
pub const A_MAX_FRAME_RETRIES: u8 = 3;

/// MAC sub-layer management entity.
///
/// All methods take `&self`: an implementation is expected to provide its own
/// interior mutability (the radio is a single shared resource). This lets the
/// upper layers hold the MAC by shared reference and drive receive and transmit
/// concurrently from separate tasks.
pub trait Mlme {
    /// The device's IEEE (extended) address as provisioned by the hardware,
    /// e.g. read from efuse.
    fn ieee_address(&self) -> IeeeAddress;

    /// MLME-SET.request of the addressing attributes (IEEE 802.15.4 7.4.2)
    /// and the operating channel.
    ///
    /// Applies what the network layer knows about this device, so the
    /// hardware filter accepts frames addressed to it: the channel it
    /// operates on, `macPANId`, and the `macShortAddress` it was assigned.
    /// [`Self::associate`] already does this for an association; the NWK layer
    /// calls this when the values come from elsewhere — a rejoin response, a
    /// parent on another channel, or a restored information base after a
    /// reset (3.6.8).
    async fn configure(&self, config: MacConfig);

    async fn scan_network(
        &self,
        ty: ScanType,
        channels: core::ops::Range<u8>,
        duration: u8,
    ) -> Result<ScanResult, MacError>;

    async fn associate(
        &self,
        channel: u8,
        dest: Address,
        capabilities: CapabilityInformation,
    ) -> Result<AssociationResponse, MacError>;

    /// MLME-POLL.request + data reception (IEEE 802.15.4 7.1.16.1).
    ///
    /// Sends a data request command to `coord_address`, then stays in
    /// receive mode to capture both the ACK (with frame-pending check)
    /// and the subsequent data frame in a single uninterrupted session.
    ///
    /// Returns `Ok((bytes_written, lqi))` on success,
    /// `Err(MacError::NoData)` if the ACK has frame-pending clear or
    /// no data frame arrives within the timeout, and `Err(MacError::NoAck)`
    /// if the data request itself went unacknowledged — the coordinator is
    /// unreachable rather than merely idle.
    async fn poll_data(
        &self,
        coord_address: Address,
        buf: &mut [u8],
    ) -> Result<(usize, u8), MacError>;

    /// Passively wait for and return the next inbound MAC data frame.
    ///
    /// Unlike [`Self::poll_data`], this sends nothing — it relies on the radio
    /// being in receive mode (rx-on-when-idle) and simply yields the next data
    /// frame addressed to this device (or a relevant broadcast). Intended for
    /// the steady-state receive loop after joining.
    ///
    /// Returns `Ok((bytes_written, lqi))` for the received frame's payload.
    async fn receive(&self, buf: &mut [u8]) -> Result<(usize, u8), MacError>;

    /// Transmit a MAC data frame carrying the given NWK-layer payload.
    ///
    /// The implementation constructs the MAC header (frame control,
    /// sequence number, addressing) and appends `payload` as the MAC
    /// service data unit.
    async fn transmit_data(&self, dest: Address, payload: &[u8]) -> Result<(), MacError>;
}

/// MAC attributes the network layer programs (see [`Mlme::configure`]).
///
/// Every field is optional: `None` leaves that attribute as the implementation
/// has it, so a caller only states what it actually knows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacConfig {
    /// `phyCurrentChannel`: the channel the network operates on.
    pub channel: Option<u8>,
    /// `macPANId` of that network.
    pub pan_id: Option<PanId>,
    /// `macShortAddress` assigned to this device.
    pub short_address: Option<ShortAddress>,
    /// `macPromiscuousMode` (7.4.2): pass every frame up instead of filtering
    /// on the addresses above.
    pub promiscuous: Option<bool>,
    /// Acknowledge received frames that request it (7.5.6.4).
    pub auto_ack_rx: Option<bool>,
    /// Wait for the acknowledgement of transmitted frames that request it.
    pub auto_ack_tx: Option<bool>,
}

impl MacConfig {
    /// Attributes of a device operating as a member of a network: filtering on
    /// its addresses, acknowledging and expecting acknowledgements.
    pub fn joined(channel: u8, pan_id: PanId, short_address: ShortAddress) -> Self {
        Self {
            channel: Some(channel),
            pan_id: Some(pan_id),
            short_address: Some(short_address),
            promiscuous: Some(false),
            auto_ack_rx: Some(true),
            auto_ack_tx: Some(true),
        }
    }
}

impl MacConfig {
    /// Tune the radio to a channel, leaving the addressing attributes alone.
    pub fn channel(channel: u8) -> Self {
        Self {
            channel: Some(channel),
            ..Self::default()
        }
    }

    /// Apply an assigned short address, leaving channel and PAN id alone.
    pub fn short_address(address: ShortAddress) -> Self {
        Self {
            short_address: Some(address),
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub struct AssociationResponse {
    pub device_address: IeeeAddress,
    pub association_address: ShortAddress,
    pub status: AssociationStatus,
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
    #[error("invalid frame")]
    InvalidFrame(byte::Error),
    #[error("no data available from coordinator")]
    NoData,
    #[error("no acknowledgment received")]
    NoAck,
    #[cfg(feature = "esp")]
    #[error("transmit failed (no ack, channel busy or coex abort)")]
    TxFailed,
    #[cfg(feature = "esp")]
    #[error("transmit-done signal timed out")]
    TxTimeout,
    #[cfg(feature = "esp")]
    #[error("radio error")]
    RadioError(#[from] esp_radio::ieee802154::Error),
    #[cfg(feature = "esp")]
    #[error("timed out waiting for the radio (held by another MAC operation)")]
    RadioLockTimeout,
    #[cfg(feature = "esp")]
    #[error("radio lock wait queue full (too many concurrent MAC operations)")]
    RadioLockQueueFull,
}

impl From<byte::Error> for MacError {
    fn from(e: byte::Error) -> Self {
        Self::InvalidFrame(e)
    }
}

#[cfg(feature = "alloc")]
pub type PanDescriptorList = alloc::vec::Vec<PanDescriptor>;
#[cfg(not(feature = "alloc"))]
pub type PanDescriptorList = heapless::Vec<PanDescriptor, MAX_PAN_DESCRIPTOR_SIZE>;

#[derive(Debug)]
pub struct ScanResult {
    pub scan_type: ScanType,
    pub pan_descriptor: PanDescriptorList,
}

#[non_exhaustive]
#[derive(Debug)]
pub struct PanDescriptor {
    pub channel: u8,
    pub coord_addr_mode: u8,
    pub coord_pan_id: ShortAddress,
    pub coord_address: Address,
    pub superframe_spec: SuperframeSpecification,
    // TODO: gts_permit, timestamp, acl_entry, security_failure
    pub link_quality: u8,
    pub security_use: bool,
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
    /// See Zigbee specification Annex D for bit field layout.
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
    // stack_profile: bits 0-3 (4 bits)
    pub const STACK_PROFILE: u16 = 0x0F;
    // protocol_version: bits 4-7 (4 bits)
    pub const PROTOCOL_VERSION: u16 = 0xF0;
    // route_capacity: bit 10 (1 bit)
    pub const ROUTER_CAPACITY: u16 = 0x400;
    // device_depth: bits 11-14 (4 bits)
    pub const DEVICE_DEPTH: u16 = 0x7800;
    // end_device_capacity: bit 15 (1 bit)
    pub const END_DEVICE_CAPACITY: u16 = 0x8000;
}
