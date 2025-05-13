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

impl<'a> TryRead<'a> for Frame<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let header: Header<'a> = bytes.read_with(offset, ())?;

        let frame = match header.frame_control.frame_type() {
            FrameType::Data => {
                let data_frame = DataFrame {
                    header,
                    payload: bytes.read_with(offset, ctx::Bytes::Len(bytes.len() - *offset))?,
                };
                Self::Data(data_frame)
            }
            FrameType::NwkCommand => {
                let cmd_frame = CommandFrame {
                    header,
                    command: bytes.read(offset)?,
                };
                Self::NwkCommand(cmd_frame)
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
    pub payload: &'a [u8],
}

/// NWK Command Frame
pub struct CommandFrame<'a> {
    pub header: Header<'a>,
    pub command: Command,
}

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
    Reserved,
}

impl TryRead<'_, ()> for Command {
    fn try_read(bytes: &'_ [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let id: u8 = bytes.read(offset)?;
        let command = match id {
            0x01 => Self::RouteRequest,
            0x02 => Self::RouteReply,
            0x03 => Self::NetworkStatus,
            0x04 => Self::Leave,
            0x05 => Self::RouteRecord,
            0x06 => Self::RejoinRequest,
            0x07 => Self::RejoinResponse,
            0x08 => Self::LinkStatus,
            0x09 => Self::NetworkReport,
            0x0a => Self::NetworkUpdate,
            0x0b => Self::EndDeviceTimeoutRequest,
            0x0c => Self::EndDeviceTimeoutResponse,
            0x0d => Self::LinkPowerDelta,
            _ => Self::Reserved,
        };

        Ok((command, *offset))
    }
}

impl TryWrite for Command {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self as u8)?;
        Ok(*offset)
    }
}
