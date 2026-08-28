use zigbee_macros::impl_byte;

impl_byte! {
    /// End Device Timeout Response Command Frame
    ///
    /// Sent by a router parent informing the end device whether it has
    /// accepted the requested timeout value and which keepalive methods it
    /// supports.
    ///
    /// See Section 3.4.12.
    #[derive(Debug, Clone)]
    pub struct EndDeviceTimeoutResponse {
        /// Whether the parent accepted the requested timeout.
        pub status: u8,
        /// Parent Information bitmask (3.6.10.3, 3.6.11.2).
        pub parent_information: u8,
    }
}

impl EndDeviceTimeoutResponse {
    /// The parent accepted the requested timeout.
    pub const STATUS_SUCCESS: u8 = 0x00;
    /// The requested timeout or configuration was out of range.
    pub const STATUS_INCORRECT_VALUE: u8 = 0x01;

    /// Parent supports the MAC data poll keepalive method.
    pub const MAC_DATA_POLL_KEEPALIVE: u8 = 1 << 0;
    /// Parent supports the End Device Timeout Request keepalive method.
    pub const TIMEOUT_REQUEST_KEEPALIVE: u8 = 1 << 1;
    /// Parent supports power negotiation (3.6.11.2).
    pub const POWER_NEGOTIATION_SUPPORTED: u8 = 1 << 2;
}

#[cfg(test)]
mod tests {
    use byte::BytesExt;
    use byte::TryRead;

    use super::*;

    #[test]
    fn roundtrip() {
        let response = EndDeviceTimeoutResponse {
            status: EndDeviceTimeoutResponse::STATUS_SUCCESS,
            parent_information: EndDeviceTimeoutResponse::MAC_DATA_POLL_KEEPALIVE
                | EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE,
        };

        let mut buf = [0u8; 2];
        buf.write_with(&mut 0, response, ()).unwrap();
        assert_eq!(buf, [0x00, 0x03]);

        let (parsed, len) = EndDeviceTimeoutResponse::try_read(&buf, ()).unwrap();
        assert_eq!(len, 2);
        assert_eq!(parsed.status, EndDeviceTimeoutResponse::STATUS_SUCCESS);
        assert_eq!(parsed.parent_information, 0x03);
    }
}
