use crate::aps::apdu::frame::command::Command;
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

impl Frame<'_> {
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
