//! APS Frame Header
use zigbee_macros::impl_byte;
use zigbee_types::ShortAddress;

use super::frame_control::DeliveryMode;
use super::frame_control::FrameControl;
use crate::aps::frame::frame_control::ExtendedFrameControlField;

impl_byte! {
    /// 2.2.5.1 General APDU Frame Format
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Header {
        pub frame_control: FrameControl,
        /// §2.2.5.1.2 — present for unicast or broadcast delivery
        #[parse_if = frame_control.has_destination_endpoint()]
        pub destination_endpoint: Option<u8>,
        /// §2.2.5.1.3 — present only for group addressing
        #[parse_if = frame_control.delivery_mode() == DeliveryMode::GroubAddressing]
        pub group_address: Option<ShortAddress>,
        /// §2.2.5.1.4 — present for data and ack frames
        #[parse_if = frame_control.has_data_fields()]
        pub cluster_id: Option<u16>,
        /// §2.2.5.1.5 — present for data and ack frames
        #[parse_if = frame_control.has_data_fields()]
        pub profile_id: Option<u16>,
        /// §2.2.5.1.6 — present for data and ack frames
        #[parse_if = frame_control.has_data_fields()]
        pub source_endpoint: Option<u8>,
        pub counter: u8,
        #[parse_if = frame_control.extended_header()]
        pub extended_header: Option<ExtendedFrameControlField>,
    }
}
