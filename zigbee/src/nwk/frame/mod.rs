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
use crate::nwk::frame::command::Command;
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

/// NWK Data Frame
pub struct DataFrame<'a> {
    pub header: Header<'a>,
    pub payload: &'a [u8],
}

/// NWK Command Frame
pub struct CommandFrame<'a> {
    pub header: Header<'a>,
    pub command: Command,
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
        //let (frame, _) = Frame::try_read(CMD_FRAME,
        // SecurityContext::no_security()).unwrap();
        // let Frame::NwkCommand(frame) = frame else {
        //    unreachable!()
        //};

        //assert!(frame.header.frame_control.security_flag());
        //assert_eq!(frame.aux_header.unwrap().security_control.0, 0x28);
    }
}
