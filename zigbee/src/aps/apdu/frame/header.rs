//! APS Frame Header
use byte::BytesExt;
use byte::TryRead;
use zigbee_macros::impl_byte;
use zigbee_types::ShortAddress;

use super::frame_control::DeliveryMode;
use super::frame_control::FrameControl;
use crate::aps::apdu::frame::frame_control::ExtendedFrameControlField;
use crate::aps::apdu::frame::frame_control::FrameType;

impl_byte! {
    /// 2.2.5.1 General APDU Frame Format
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Header {
        pub frame_control: FrameControl,
        #[parse_if = frame_control.ack_format_flag()]
        pub destination: Option<ShortAddress>,
        #[parse_if = frame_control.delivery_mode() == DeliveryMode::GroubAddressing]
        pub group_address: Option<ShortAddress>,
        #[parse_if = frame_control.ack_format_flag()]
        pub cluster_id: Option<u8>,
        #[parse_if = frame_control.ack_format_flag()]
        pub profile_id: Option<u8>,
        #[parse_if = frame_control.ack_format_flag()]
        pub source_endpoint: Option<u8>,
        pub counter: u8,
        #[parse_if = frame_control.extended_header()]
        pub extended_header: Option<ExtendedFrameControlField>,
    }
}
