//! ZCL Header
#![allow(dead_code, unreachable_pub)]

pub mod command_identifier;
pub mod frame_control;
pub mod manufacturer_code;

use core::fmt::Debug;

use command_identifier::CommandIdentifier;
use frame_control::FrameControl;
use manufacturer_code::ManufacturerCode;
use zigbee_macros::impl_byte;

impl_byte! {
    /// 2.4.1 ZCL Header
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ZclHeader {
        /// See Section 2.4.1.1.
        pub frame_control: FrameControl,
        /// See Section 2.4.1.2.
        #[parse_if = frame_control.is_manufacturer_specific()]
        pub manufacturer_code: Option<ManufacturerCode>,
        /// See Section 2.4.1.3.
        pub sequence_number: u8,
        /// See Section 2.4.1.4.
        pub command_identifier: CommandIdentifier,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;
    use crate::header::frame_control::FrameType;

    #[test]
    fn unpack_header_without_manufacturer_code() {
        // given
        let input = [0x18, 0x01, 0x0a];

        // when
        let (header, _) =
            ZclHeader::try_read(&input, ()).expect("Could not read ZclHeader in test");

        // then
        assert_eq!(header.frame_control.frame_type(), FrameType::GlobalCommand);
        assert!(!header.frame_control.is_manufacturer_specific());
        assert_eq!(header.manufacturer_code, None);
        assert_eq!(header.sequence_number, 1);
        assert_eq!(
            header.command_identifier,
            CommandIdentifier::ReportAttributes
        );
    }

    #[test]
    fn unpack_header_with_manufacturer_code() {
        // given
        let input = [0x1c, 0x11, 0x12, 0x02, 0x0a];

        // when
        let (header, _) =
            ZclHeader::try_read(&input, ()).expect("Could not read ZclHeader in test");

        // then
        assert_eq!(header.frame_control.frame_type(), FrameType::GlobalCommand);
        assert!(header.frame_control.is_manufacturer_specific());
        assert_eq!(header.manufacturer_code, Some(ManufacturerCode(4625)));
        assert_eq!(header.sequence_number, 2);
        assert_eq!(
            header.command_identifier,
            CommandIdentifier::ReportAttributes
        );
    }
}
