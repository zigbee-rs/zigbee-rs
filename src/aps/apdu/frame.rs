//! APS Header Frame Control
use core::mem;

use crate::impl_byte;

impl_byte! {
    /// Frame Control field
    ///
    /// See Section 2.2.5.1.1
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct FrameControl(pub u8);
}

impl FrameControl {
    /// See Section 2.2.5.1.1
    pub fn frame_type(&self) -> FrameType {
        unsafe { mem::transmute((self.0 & mask::FRAME_TYPE) >> offset::FRAME_TYPE) }
    }

    /// Sets the frame type
    #[must_use]
    pub fn set_frame_type(mut self, value: FrameType) -> Self {
        self.0 = (self.0 & !mask::FRAME_TYPE) | (value as u8) << offset::FRAME_TYPE;
        self
    }

    pub fn delivery_mode(&self) -> DeliveryMode {
        unsafe { mem::transmute((self.0 & mask::DELIVERY_MODE) >> offset::DELIVERY_MODE) }
    }

    /// indicates if the destination endpoint, cluster identifier, profile
    /// identifier and source endpoint fields shall be  present in the
    /// acknowledgement frame.
    pub fn ack_format_flag(&self) -> bool {
        ((self.0 & mask::ACK_FORMAT_FLAG) >> offset::ACK_FORMAT_FLAG) != 0
    }

    pub fn security_flag(&self) -> bool {
        ((self.0 & mask::SECURITY_FLAG) >> offset::SECURITY_FLAG) != 0
    }

    // specifies whether the current transmission requires an  acknowledgement frame
    // to be sent to the originator on receipt of the frame
    //
    // This sub-field shall be set to 0 for all frames that are broadcast or
    // multicast.
    pub fn ack_request(&self) -> bool {
        ((self.0 & mask::ACK_FLAG) >> offset::ACK_FLAG) != 0
    }

    // specifies whether the extended header shall be included  in the frame.
    // If this sub-field is set to 1, then the extended header shall be included in
    // the frame. Otherwise, it shall not  be included in the frame.
    pub fn extended_header(&self) -> bool {
        ((self.0 & mask::EXTENDED_HEADER_FLAG) >> offset::EXTENDED_HEADER_FLAG) != 0
    }
}

impl core::fmt::Debug for FrameControl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrameControl")
            .field("frame_type", &self.frame_type())
            .field("delivery_mode", &self.delivery_mode())
            .field("ack_format", &self.ack_format_flag())
            .field("security_flag", &self.security_flag())
            .field("ack_request", &self.ack_request())
            .field("extended_header", &self.extended_header())
            .finish()
    }
}

/// Frame Type
///
/// See Section 2.2.5.1.1
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Data = 0b00,
    Command = 0b01,
    Acknowledtement = 0b10,
    InterPan = 0b11,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    Unicast = 0b00,
    Reserved = 0b01,
    Broadcast = 0b10,
    GroubAddressing = 0b11,
}

mod offset {
    pub const FRAME_TYPE: u8 = 0;
    pub const DELIVERY_MODE: u8 = 1;
    pub const ACK_FORMAT_FLAG: u8 = 1;
    pub const SECURITY_FLAG: u8 = 1;
    pub const ACK_FLAG: u8 = 1;
    pub const EXTENDED_HEADER_FLAG: u8 = 1;
}

mod mask {
    pub const FRAME_TYPE: u8 = 0x1;
    pub const DELIVERY_MODE: u8 = 0x2;
    pub const ACK_FORMAT_FLAG: u8 = 0x3;
    pub const SECURITY_FLAG: u8 = 0x4;
    pub const ACK_FLAG: u8 = 0x5;
    pub const EXTENDED_HEADER_FLAG: u8 = 0x6;
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;

    #[test]
    fn parse_frame_control() {
        let raw = [0b0010_0001_u8];

        let (frame_control, len) = FrameControl::try_read(&raw, ()).unwrap();
        assert_eq!(len, 1);
        assert_eq!(frame_control.frame_type(), FrameType::Command);
        assert_eq!(frame_control.delivery_mode(), DeliveryMode::Unicast);
        assert!(!frame_control.ack_format_flag());
        assert!(!frame_control.security_flag());
        assert!(!frame_control.ack_request());
        assert!(!frame_control.extended_header());
    }
}

