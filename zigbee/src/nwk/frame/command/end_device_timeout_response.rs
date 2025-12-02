use zigbee_macros::impl_byte;

impl_byte! {
    /// End Device Timeout Response Command Frame
    #[derive(Debug, Clone)]
    pub struct EndDeviceTimeoutResponse {
        pub timeout: u8,
    }
}
