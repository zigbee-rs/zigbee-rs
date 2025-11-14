use zigbee_macros::impl_byte;

impl_byte! {
    /// Network Update Command Frame
    #[derive(Debug, Clone)]
    pub struct NetworkUpdate {
        pub update_id: u8,
        pub channel: u8,
        pub pan_id: u16,
        pub network_address: u16,
    }
}
