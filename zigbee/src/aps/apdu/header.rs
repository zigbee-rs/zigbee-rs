//! APS Frame Header
use byte::BytesExt;
use byte::TryRead;

use super::extended_frame_control::ExtendedFrameControlField;
use super::frame::DeliveryMode;
use super::frame::FrameControl;
use crate::aps::apdu::frame::FrameType;
use crate::impl_byte;
use crate::internal::types::ShortAddress;

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
