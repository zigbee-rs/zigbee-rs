//! APS Header
use crate::{aps::frame::frame_control::{DeliveryMode, FrameType}, common::types::ShortAddress, impl_byte};

use super::frame_control::FrameControl;

impl_byte! {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Header {
        /// See Section 2.2.5.1.1.
        pub frame_control: FrameControl,
        /// See Section 2.2.5.1.2.
        #[parse_if = frame_control.delivery_mode() == DeliveryMode::NormalUnicast]
        pub destination: Option<u8>,
        /// See Section 2.2.5.1.3.
        #[parse_if = frame_control.delivery_mode() == DeliveryMode::GroupAddressing]
        pub group_address: Option<u16>,
        /// See Section 2.2.5.1.4.
        #[parse_if = frame_control.frame_type() == FrameType::Data || frame_control.frame_type() == FrameType::Acknowledgement ]
        pub cluster_identifier: Option<u16>,
        /// See Section 2.2.5.1.5.
        #[parse_if = frame_control.frame_type() == FrameType::Data || frame_control.frame_type() == FrameType::Acknowledgement ]
        pub profile_identifier: Option<u16>,
        /// See Section 2.2.5.1.6.
        pub source_endpoint: u8,
        /// See Section 2.2.5.1.7.
        pub aps_counter: u8,
        /// See Section 2.2.5.1.8.
        #[parse_if = frame_control.has_extended_header()]
        pub extended_header: Option<ExtendedHeader>,
    }
}


impl_byte! {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ExtendedHeader(u8);
}

