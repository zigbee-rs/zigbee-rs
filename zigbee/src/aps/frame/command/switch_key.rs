use zigbee_macros::impl_byte;

impl_byte! {
    /// Switch-Key Command Frame (4.4.10.5, Figure 4-14, command id 0x09)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SwitchKey {
        /// Sequence number of the network key to activate
        pub sequence_number: u8,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use crate::aps::frame::command::Command;

    #[test]
    fn round_trip_switch_key() {
        let frame_buf = [
            0x09, // command id: SwitchKey
            0x07, // sequence number
        ];

        let (cmd, _) = Command::try_read(&frame_buf, ()).unwrap();

        let mut got_buf = [0u8; _];
        cmd.try_write(&mut got_buf, ()).unwrap();

        assert_eq!(frame_buf, got_buf);
    }
}
