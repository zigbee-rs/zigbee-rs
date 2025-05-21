use crate::common::types::ShortAddress;

pub struct NetworkStatus {
    pub status_code: NetworkStatusCode,
    pub destination_address: ShortAddress,
}

/// Network Status Codes
///
/// See Section 3.4.3.3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NetworkStatusCode {
    /// No route available
    NoRouteAvailable = 0x00,
    /// Tree link failure
    TreeLinkFailure = 0x01,
    /// Non-tree link failure
    NonTreeLinkFailure = 0x02,
    /// Low battery level
    LowBatteryLevel = 0x03,
    /// No routing capacity
    NoRoutingCapacity = 0x04,
    /// No indirect capacity
    NoIndirectCapacity = 0x05,
    /// Indirect transaction expiry
    IndirectTransactionExpiry = 0x06,
    /// Target device unavailable
    TargetDeviceUnavailable = 0x07,
    /// Target address unallocated
    TargetAddressUnallocated = 0x08,
    /// Parent link failure
    ParentLinkFailure = 0x09,
    /// Validate route
    ValidateRoute = 0x0a,
    /// Source route failure
    SourceRouteFailure = 0x0b,
    /// Many-to-one route failure
    ManyToOneRouteFailure = 0x0c,
    /// Address conflict
    AddressConflict = 0x0d,
    /// Verify addresses
    VerifyAddresses = 0x0e,
    /// PAN identifier update
    PanIdentifierUpdate = 0x0f,
    /// Network address update
    NetworkAddressUpdate = 0x10,
    /// Bad frame counter
    BadFrameCounter = 0x11,
    /// Bad key sequence number
    BadKeySequenceNumber = 0x12,
    /// Reserved
    Reserved,
}

impl NetworkStatusCode {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x00 => Self::NoRouteAvailable,
            0x01 => Self::TreeLinkFailure,
            0x02 => Self::NonTreeLinkFailure,
            0x03 => Self::LowBatteryLevel,
            0x04 => Self::NoRoutingCapacity,
            0x05 => Self::NoIndirectCapacity,
            0x06 => Self::IndirectTransactionExpiry,
            0x07 => Self::TargetDeviceUnavailable,
            0x08 => Self::TargetAddressUnallocated,
            0x09 => Self::ParentLinkFailure,
            0x0a => Self::ValidateRoute,
            0x0b => Self::SourceRouteFailure,
            0x0c => Self::ManyToOneRouteFailure,
            0x0d => Self::AddressConflict,
            0x0e => Self::VerifyAddresses,
            0x0f => Self::PanIdentifierUpdate,
            0x10 => Self::NetworkAddressUpdate,
            0x11 => Self::BadFrameCounter,
            0x12 => Self::BadKeySequenceNumber,
            _ => Self::Reserved,
        }
    }
}
