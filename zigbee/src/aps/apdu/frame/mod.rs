use byte::BytesExt;

use crate::aps::apdu::frame::command::Command;
use crate::aps::apdu::frame::frame_control::FrameType;
use crate::aps::apdu::frame::header::Header;

pub mod command;
pub mod frame_control;
pub mod header;

/// APS Frame
#[derive(Debug)]
pub enum Frame<'a> {
    /// Data Frame
    Data(DataFrame<'a>),
    /// Command Frame
    ApsCommand(CommandFrame),
    /// Ack
    Acknowledgement(Header),
}

impl<'a> Frame<'a> {
    pub fn from_payload(header: Header, payload: &'a [u8]) -> byte::Result<Self> {
        match header.frame_control.frame_type() {
            FrameType::Data => Ok(Self::Data(DataFrame { header, payload })),
            FrameType::Command => Ok(Self::ApsCommand(CommandFrame {
                header,
                command: payload.read_with(&mut 0, ())?,
            })),
            FrameType::Acknowledgement => Ok(Self::Acknowledgement(header)),
            FrameType::InterPan => unimplemented!("InterPan frames not supported"),
        }
    }

    pub fn header(&self) -> &Header {
        match self {
            Frame::Data(data_frame) => &data_frame.header,
            Frame::ApsCommand(command_frame) => &command_frame.header,
            Frame::Acknowledgement(header) => header,
        }
    }
}

#[derive(Debug)]
pub struct DataFrame<'a> {
    pub header: Header,
    pub payload: &'a [u8],
}

#[derive(Debug)]
pub struct CommandFrame {
    pub header: Header,
    pub command: Command,
}
