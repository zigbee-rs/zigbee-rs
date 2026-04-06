use zigbee_macros::impl_byte;
use zigbee_types::IeeeAddress;

impl_byte! {
    /// Confirm-Key Command Frame (§4.4.10.9, Table 4-27, command id 0x10)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConfirmKey {
        /// Status of the verify-key operation (0x00 = success)
        pub status: u8,
        /// Standard key type (0x04 = Trust Center Link Key)
        pub key_type: u8,
        /// IEEE address of the destination (the joining node)
        pub destination_address: IeeeAddress,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use crate::aps::frame::command::Command;

    #[test]
    fn round_trip_confirm_key() {
        let frame_buf = [
            0x10, // command id: ConfirmKey
            0x00, // status: success
            0x04, // key_type: TC Link Key
            // destination_address (8 bytes LE)
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        ];

        let (cmd, _) = Command::try_read(&frame_buf, ()).unwrap();

        let mut got_buf = [0u8; _];
        cmd.try_write(&mut got_buf, ()).unwrap();

        assert_eq!(frame_buf, got_buf);
    }
}
