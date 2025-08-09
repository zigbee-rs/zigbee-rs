use crate::impl_byte;
use crate::internal::types::ByteArray;
use crate::internal::types::IeeeAddress;

impl_byte! {
    #[tag(u8)]
    /// Transport Key Command Frame
    /// 4.4.10.1
    /// Table 4-9
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TransportKey {
        #[tag_value = 0x01]
        StandardNetworkKey(StandardNetworkKeyDescriptor),
        #[tag_value = 0x03]
        ApplicationLinkKey(ApplicationLinkKeyDescriptor),
        #[tag_value = 0x04]
        TrustCenterLinkKey(TrustCenterLinkKeyDescriptor),
        #[fallback = true]
        Reserved(u8),
    }
}

impl_byte! {
    /// Figure 4-8
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TrustCenterLinkKeyDescriptor {
        pub key: ByteArray<16>,
        pub destination_address: IeeeAddress,
        pub source_address: IeeeAddress,
    }
}

impl_byte! {
    /// Figure 4-9
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StandardNetworkKeyDescriptor {
        pub key: ByteArray<16>,
        pub sequence_number: u8,
        pub destination_address: IeeeAddress,
        pub source_address: IeeeAddress,
    }
}

impl_byte! {
    /// Figure 4-10
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ApplicationLinkKeyDescriptor {
        pub key: ByteArray<16>,
        pub partner_address: IeeeAddress,
        #[ctx = ()]
        pub initiator_flag: bool,
    }
}
