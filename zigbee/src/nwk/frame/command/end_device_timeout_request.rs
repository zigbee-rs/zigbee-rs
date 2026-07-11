use zigbee_macros::impl_byte;

impl_byte! {
    /// End Device Timeout Request Command Frame
    ///
    /// Sent by an end device informing its parent of its timeout
    /// requirements so the parent can age the child entry accordingly.
    ///
    /// See Section 3.4.11.
    #[derive(Debug, Clone)]
    pub struct EndDeviceTimeoutRequest {
        /// Requested timeout enumeration (0–14, Table 3.52).
        pub requested_timeout: u8,
        /// Requested configuration bitmask; all bits reserved, must be 0
        /// (§3.4.11.3.2).
        pub end_device_configuration: u8,
    }
}

/// Convert a Requested Timeout Enumeration value into seconds (Table 3.52).
///
/// Returns `None` for values outside the valid range 0–14.
pub fn timeout_seconds(enum_value: u8) -> Option<u32> {
    match enum_value {
        0 => Some(10),
        1..=14 => Some(120 << (enum_value - 1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use byte::BytesExt;
    use byte::TryRead;

    use super::*;

    #[test]
    fn timeout_seconds_table() {
        assert_eq!(timeout_seconds(0), Some(10));
        assert_eq!(timeout_seconds(1), Some(120));
        assert_eq!(timeout_seconds(8), Some(15_360)); // 256 min
        assert_eq!(timeout_seconds(14), Some(983_040)); // 16384 min
        assert_eq!(timeout_seconds(15), None);
    }

    #[test]
    fn roundtrip() {
        let request = EndDeviceTimeoutRequest {
            requested_timeout: 0x08,
            end_device_configuration: 0x00,
        };

        let mut buf = [0u8; 2];
        buf.write_with(&mut 0, request, ()).unwrap();
        assert_eq!(buf, [0x08, 0x00]);

        let (parsed, len) = EndDeviceTimeoutRequest::try_read(&buf, ()).unwrap();
        assert_eq!(len, 2);
        assert_eq!(parsed.requested_timeout, 0x08);
        assert_eq!(parsed.end_device_configuration, 0x00);
    }
}
