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

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use crate::aps::apdu::frame::command::Command;

    #[test]
    fn parse_transport_key() {
        let frame_buf = [
            0x5, 0x1, 0xab, 0xcd, 0xef, 0x1, 0x23, 0x45, 0x67, 0x89, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0xe5, 0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1, 0xa4, 0xe1, 0x52, 0x38, 0x7d,
            0xc1, 0x36, 0xce, 0xf4,
        ];

        let (frame, _) = Command::try_read(&frame_buf, ()).unwrap();

        let mut got_buf = [0u8; _];
        frame.try_write(&mut got_buf, ()).unwrap();

        assert_eq!(frame_buf, got_buf);
    }
}
