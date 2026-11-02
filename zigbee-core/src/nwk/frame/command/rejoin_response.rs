use zigbee_macros::impl_byte;
use zigbee_types::ShortAddress;

impl_byte! {
    /// Rejoin Response Command Frame
    #[derive(Debug, Clone)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct RejoinResponse {
        pub network_address: ShortAddress,
        pub status: u8,
    }
}
