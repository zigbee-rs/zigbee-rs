use crate::common::types::ShortAddress;
use crate::impl_byte;

pub struct NetworkStatus {
    pub status_code: NetworkStatusCode,
    pub destination_address: ShortAddress,
}

impl_byte! {
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
        #[fallback = true]
        Reserved,
    }
}
