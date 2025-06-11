//! APS Header
use core::mem;

use crate::impl_byte;


impl_byte! {
    /// Frame Control field
    ///
    /// See Section 2.2.5.1.1.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct FrameControl(pub u8);
}

impl core::fmt::Debug for FrameControl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrameControl")
            .field("frame_type", &self.frame_type())
            .field("delivery_mode", &self.delivery_mode())
            .field("acknowledgement_format", &self.acknowledgement_format())
            .field("security_flag", &self.security_flag())
            .field("requires_acknowledgement", &self.requires_acknowledgement())
            .field("has_extended_header", &self.has_extended_header())
            .finish()
    }
}


impl FrameControl {
    pub fn frame_type(&self) -> FrameType {
        // SAFETY: any 2 bit permutation is a valid FrameType
        unsafe { mem::transmute((self.0 & mask::FRAME_TYPE) >> offset::FRAME_TYPE) }
        // unsafe { mem::transmute(((self.0 & mask::FRAME_TYPE) >> offset::FRAME_TYPE) as u8) }
    }

    /// Sets the frame type
    #[must_use]
    pub fn set_frame_type(mut self, value: FrameType) -> Self {
        self.0 |= (value as u8) << offset::FRAME_TYPE;
        self
    }

    pub fn delivery_mode(&self) -> DeliveryMode {
        // SAFETY: any 2 bit permutation is a valid DeliveryMode
        unsafe { mem::transmute((self.0 & mask::DELIVERY_MODE) >> offset::DELIVERY_MODE) }
    }

    /// Sets the delivery mode
    #[must_use]
    pub fn set_delivery_mode(mut self, value: DeliveryMode) -> Self {
        self.0 |= (value as u8) << offset::DELIVERY_MODE;
        self
    }

    pub fn acknowledgement_format(self) -> bool {
        ((self.0 & mask::ACK_FORMAT) >> offset::ACK_FORMAT) != 0
    }

    pub fn security_flag(self) -> bool {
        ((self.0 & mask::SECURITY) >> offset::SECURITY) != 0
    }

    pub fn requires_acknowledgement(self) -> bool {
        ((self.0 & mask::ACK_REQUEST) >> offset::ACK_REQUEST) != 0
    }

    pub fn has_extended_header(self) -> bool {
        ((self.0 & mask::EXTENDED_HEADER) >> offset::EXTENDED_HEADER) != 0
    }

}

mod mask {
    pub const FRAME_TYPE: u8 = 0b0000_0011;
    pub const DELIVERY_MODE: u8 = 0b0000_1100;
    pub const ACK_FORMAT: u8 = 0b0001_0000;
    pub const SECURITY: u8 = 0b0010_0000;
    pub const ACK_REQUEST: u8 = 0b0100_0000;
    pub const EXTENDED_HEADER: u8 = 0b1000_0000;
}

mod offset {
    pub const FRAME_TYPE: u8 = 0;
    pub const DELIVERY_MODE: u8 = 2;
    pub const ACK_FORMAT: u8 = 4;
    pub const SECURITY: u8 = 5;
    pub const ACK_REQUEST: u8 = 6;
    pub const EXTENDED_HEADER: u8 = 7;
}

/// Frame Type
///
/// See Section 2.2.5.1.1.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Data = 0b00,
    Command = 0b01,
    Acknowledgement = 0b10,
    InterPan = 0b11,
}

/// Delivery Mode
///
/// See Section 2.2.5.1.1.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    NormalUnicast = 0b00,
    Reserved = 0b01,
    Broadcast = 0b10,
    GroupAddressing = 0b11,
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;

    #[test]
    fn parse_frame_control() {
        // given
        let input: &[u8] = &[0b1010_1001_u8];
        //                     .      ^^ frame_type
        //                     .    ^^   delivery_mode
        //                     .  ^      ack format
        //                     . ^       security_flag flag
        //                     .^        requires_acknowledgement flag
        //                     ^         has_extended_header flag

        // when
        let (frame_control, len) = FrameControl::try_read(input, ()).unwrap();

        // then
        assert_eq!(len, 1);
        assert_eq!(frame_control.frame_type(), FrameType::Command);
        assert_eq!(frame_control.delivery_mode(), DeliveryMode::Broadcast);
        assert!(!frame_control.acknowledgement_format());
        assert!(frame_control.security_flag());
        assert!(!frame_control.requires_acknowledgement());
        assert!(frame_control.has_extended_header());
    }
}

