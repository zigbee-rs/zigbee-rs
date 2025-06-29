//! NWK Frame Formats
pub mod command;
pub mod frame_control;
pub mod header;

use byte::ctx;
use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use frame_control::FrameType;
use header::Header;

use crate::internal::macros::impl_byte;
use crate::security::frame::AuxFrameHeader;
use crate::security::SecurityContext;

/// NWK Frame
pub enum Frame<'a> {
    /// Data Frame
    Data(DataFrame<'a>),
    /// Command Frame
    NwkCommand(CommandFrame<'a>),
    /// Reserved
    Reserved(Header<'a>),
    /// Inter-Pan
    InterPan(Header<'a>),
}

impl<'a> TryRead<'a, SecurityContext> for Frame<'a> {
    fn try_read(bytes: &'a [u8], cx: SecurityContext) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let header: Header<'a> = bytes.read_with(offset, ())?;

        let has_security = header.frame_control.security_flag();

        let frame = match header.frame_control.frame_type() {
            FrameType::Data => {
                unimplemented!()
            }
            FrameType::NwkCommand => {
                unimplemented!()
            }
            FrameType::Reserved => Self::Reserved(header),
            FrameType::InterPan => Self::InterPan(header),
        };

        Ok((frame, *offset))
    }
}

/// NWK Data Frame
pub struct DataFrame<'a> {
    pub header: Header<'a>,
    pub aux_header: Option<AuxFrameHeader>,
    pub payload: &'a [u8],
}

/// NWK Command Frame
pub struct CommandFrame<'a> {
    pub header: Header<'a>,
    pub aux_header: Option<AuxFrameHeader>,
    pub command: Command,
}

impl_byte! {
    /// Comand Frame Identifiers.
    ///
    /// See Section 3.4.
    #[derive(Debug)]
    #[repr(u8)]
    pub enum Command {
        RouteRequest = 0x01,
        RouteReply = 0x02,
        NetworkStatus = 0x03,
        Leave = 0x04,
        RouteRecord = 0x05,
        RejoinRequest = 0x06,
        RejoinResponse = 0x07,
        LinkStatus = 0x08,
        NetworkReport = 0x09,
        NetworkUpdate = 0x0a,
        EndDeviceTimeoutRequest = 0x0b,
        EndDeviceTimeoutResponse = 0x0c,
        LinkPowerDelta = 0x0d,
        #[fallback = true]
        Reserved,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;
    use crate::security::SecurityContext;

    const CMD_FRAME: &[u8] = &[
        0x09, 0x12, // frame control
        0xff, 0xff, // destination,
        0x34, 0x12, // src
        0x01, // radius
        0xaa, // seq number
        0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, // ext src
        0x28, //sec header
        0xff, 0xff, 0xff, 0xff, // frame counter
        0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, // ext src
        0x01, // key seq
        0x01, // command id
    ];

    #[test]
    fn command_with_security() {
        let (frame, _) = Frame::try_read(CMD_FRAME, SecurityContext::no_security()).unwrap();
        let Frame::NwkCommand(frame) = frame else {
            unreachable!()
        };

        assert!(frame.header.frame_control.security_flag());
        assert_eq!(frame.aux_header.unwrap().security_control.0, 0x28);
    }
}
