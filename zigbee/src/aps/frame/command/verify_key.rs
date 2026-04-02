use zigbee_macros::impl_byte;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;

impl_byte! {
    /// Verify-Key Command Frame (§4.4.10.8, Table 4-27, command id 0x0f)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VerifyKey {
        /// Standard key type (0x04 = Trust Center Link Key)
        pub key_type: u8,
        /// IEEE address of the Trust Center
        pub source_address: IeeeAddress,
        /// HMAC-AES-128-MMO hash proving possession of the link key
        pub hash: ByteArray<16>,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use crate::aps::frame::command::Command;

    #[test]
    fn round_trip_verify_key() {
        let frame_buf = [
            0x0f, // command id: VerifyKey
            0x04, // key_type: TC Link Key
            // source_address (8 bytes LE)
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            // hash (16 bytes)
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11,
            0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
        ];

        let (cmd, _) = Command::try_read(&frame_buf, ()).unwrap();

        let mut got_buf = [0u8; _];
        cmd.try_write(&mut got_buf, ()).unwrap();

        assert_eq!(frame_buf, got_buf);
    }
}
