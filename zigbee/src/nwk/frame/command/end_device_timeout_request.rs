use crate::internal::macros::impl_byte;

impl_byte! {
    /// End Device Timeout Request Command Frame
    #[derive(Debug, Clone)]
    pub struct EndDeviceTimeoutRequest {
        pub requested_timeout: u8,
        pub end_device_configuration: u8,
    }
}
