//! APS Frame Header
use byte::{BytesExt, TryRead};

use super::{extended_frame_control::ExtendedFrameControlField, frame::{DeliveryMode, FrameControl}};
use crate::common::types::ShortAddress;

/// 2.2.5.1 General APDU Frame Format
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub frame_control: FrameControl,
    pub destination: Option<ShortAddress>,
    pub group_address: Option<ShortAddress>,
    pub cluster_id: Option<u8>,
    pub profile_id: Option<u8>,
    pub source_endpoint: Option<ShortAddress>,
    pub counter: u8,
    pub extended_header: ExtendedFrameControlField,
}

impl TryRead<'_, ()> for Header {
    fn try_read(bytes: &'_ [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let frame_control: FrameControl = bytes.read_with(offset, ())?;

        let destination: Option<ShortAddress> = if frame_control.delivery_mode() == DeliveryMode::Unicast {
            Some(ShortAddress(bytes.read_with(offset, ())?)) 
        } else {
            None
        };

        let group_address: Option<ShortAddress> = if frame_control.delivery_mode() == DeliveryMode::GroubAddressing {
            Some(ShortAddress(bytes.read_with(offset, ())?))
        } else {
            None
        };

        let cluster_id: Option<u8> = match frame_control.frame_type() {
            super::frame::FrameType::Data |
            super::frame::FrameType::Acknowledtement => {
                Some(bytes.read_with(offset, ())?)
            },
            _ => None
        };

        let profile_id: Option<u8> = match frame_control.frame_type() {
            super::frame::FrameType::Data |
            super::frame::FrameType::Acknowledtement => {
                Some(bytes.read_with(offset, ())?)
            },
            _ => None
        };

        let source_endpoint = Some(bytes.read_with(offset, ())?);

        let counter: u8 = bytes.read_with(offset, ())?;

        let extended_header: ExtendedFrameControlField  = bytes.read_with(offset, ())?;

        let header = Self {
            frame_control,
            destination,
            group_address,
            cluster_id,
            profile_id,
            source_endpoint,
            counter,
            extended_header,
        };

        Ok((header, *offset))
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;
    use crate::aps::apdu::frame::FrameType;

    #[test]
    fn parse_aps_header() {
        // given
        let raw = [
            0x21, 0x95, 0x30, 0x00, 0x00, 0x00, 0x00, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce,
            0xf4, 0xcc, 0x56, 0x50, 0x5e, 0x07, 0x2d, 0xc5, 0xc1, 0xe8, 0x40, 0xf2, 0xd5, 0xce,
            0x0c, 0xa9, 0x2d, 0x64, 0x23, 0xcc, 0x0c, 0x56, 0xcc, 0xc4, 0xcc, 0x0f, 0x18, 0xa2,
            0xe4, 0x82, 0x88, 0x58, 0x4a, 0x90, 0x3e, 0x00, 0x47, 0x60, 0xf2, 0x5d,
        ];

        // when
        let (header, _) = Header::try_read(&raw, ()).unwrap();

        // then
        assert_eq!(header.frame_control.frame_type(), FrameType::Command);
        // assert!(header.frame_control.delivery_mode());
        // assert_eq!(header.ack_format, ShortAddress(0xfffc));
        // assert_eq!(header.security,
        // Some(IeeeAddress(0x0012_4b00_2a9a_7166))); assert_eq!(header.
        // ack_request, 8);
    }
}
