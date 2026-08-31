//! NWK Frame Formats
pub mod command;
pub mod frame_control;
pub mod header;

use core::mem;
use core::slice;

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::ctx;
use frame_control::FrameType;
use header::Header;
use zigbee_macros::impl_byte;

use crate::nwk::frame::command::Command;
use crate::security::SecurityContext;
use crate::security::frame::AuxFrameHeader;

/// NWK Frame
#[derive(Debug, Clone)]
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

impl<'a> Frame<'a> {
    pub fn from_payload(header: Header<'a>, payload: &'a [u8]) -> byte::Result<Self> {
        match header.frame_control.frame_type() {
            FrameType::Data => {
                let data_frame = DataFrame { header, payload };
                Ok(Frame::Data(data_frame))
            }
            FrameType::NwkCommand => {
                let command_frame = CommandFrame {
                    header,
                    command: payload.read_with(&mut 0, ())?,
                };
                Ok(Frame::NwkCommand(command_frame))
            }
            FrameType::Reserved => Ok(Frame::Reserved(header)),
            FrameType::InterPan => Ok(Frame::InterPan(header)),
        }
    }
}

impl<'a> TryRead<'a, SecurityContext<'a>> for Frame<'a> {
    #[allow(clippy::ptr_cast_constness, clippy::as_ptr_cast_mut)]
    fn try_read(bytes: &'a [u8], cx: SecurityContext) -> byte::Result<(Self, usize)> {
        let len = bytes.len();
        // SAFETY: the slice is not used afterwards, so casting away constness
        // is sound
        let bytes: &'a mut [u8] =
            unsafe { slice::from_raw_parts_mut(bytes.as_ptr() as *mut u8, len) };
        let frame = cx.decrypt_nwk_frame_in_place(bytes)?;
        // the full frame is always consumed; the "rest" lives in frame.payload
        Ok((frame, len))
    }
}

impl<'a> TryWrite<SecurityContext<'a>> for Frame<'a> {
    fn try_write(self, bytes: &mut [u8], cx: SecurityContext) -> byte::Result<usize> {
        let len = cx.encrypt_nwk_frame_in_place(self, bytes)?;
        Ok(len)
    }
}

/// NWK Data Frame
#[derive(Debug, Clone)]
pub struct DataFrame<'a> {
    pub header: Header<'a>,
    pub payload: &'a [u8],
}

impl<'a> DataFrame<'a> {
    // SAFETY: caller must ensure the buffer referenced by `self` is mutable
    // locally
    pub(crate) unsafe fn payload_as_mut(&mut self) -> &'a mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.payload.as_ptr().cast_mut(), self.payload.len()) }
    }
}

/// NWK Command Frame
#[derive(Debug, Clone)]
pub struct CommandFrame<'a> {
    pub header: Header<'a>,
    pub command: Command<'a>,
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
        // TODO: parse CMD_FRAME with a real SecurityContext and assert the
        // decrypted command frame's security flag and aux header
    }
}
