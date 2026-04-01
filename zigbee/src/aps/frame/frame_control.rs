//! APS Header Frame Control
//!
//! See Section 2.2.5.1
use core::mem;

use zigbee_macros::impl_byte;

impl_byte! {
    /// Frame Control field
    ///
    /// See Section 2.2.5.1.1
    #[derive(Clone, Copy, Eq, PartialEq, Default)]
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

    #[must_use]
    pub fn set_delivery_mode(mut self, value: DeliveryMode) -> Self {
        self.0 = (self.0 & !mask::DELIVERY_MODE) | (value as u8) << offset::DELIVERY_MODE;
        self
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

    /// Whether the endpoint, cluster, and profile fields are present (§2.2.5.1).
    ///
    /// True for data frames and for ack frames with ack_format set.
    pub fn has_data_fields(&self) -> bool {
        matches!(self.frame_type(), FrameType::Data | FrameType::InterPan)
            || (self.frame_type() == FrameType::Acknowledgement && self.ack_format_flag())
    }

    /// Whether the destination endpoint field is present (§2.2.5.1.2).
    ///
    /// Present for unicast or broadcast delivery when data fields are included.
    pub fn has_destination_endpoint(&self) -> bool {
        self.has_data_fields()
            && matches!(
                self.delivery_mode(),
                DeliveryMode::Unicast | DeliveryMode::Broadcast
            )
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
    Acknowledgement = 0b10,
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

impl_byte! {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ExtendedFrameControlField {
        pub extended_frame_control: ExtendedFrameControl,
        #[parse_if = extended_frame_control.is_fragmented()]
        pub block_number: Option<u8>,
        #[parse_if = extended_frame_control.is_fragmented()]
        pub ack_bitfield: Option<u8>,
    }
}

impl_byte! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ExtendedFrameControl {
        fragmentation: Fragmentation
    }
}

impl ExtendedFrameControl {
    pub fn is_fragmented(&self) -> bool {
        matches!(
            self.fragmentation,
            Fragmentation::Fragmentation | Fragmentation::PartOfFragmentedTransmission
        )
    }
}

impl_byte! {
    #[tag(u8)]
    /// Extended Header Sub-Frame
    ///
    /// See Section 2.2.5.1.8
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Fragmentation {
        NoFragmentation = 0b00,
        Fragmentation = 0b01,
        PartOfFragmentedTransmission = 0b10,
        #[fallback = true]
        Reserved(u8),
    }
}

// §2.2.5.1.1 frame control bit layout
mod offset {
    pub const FRAME_TYPE: u8 = 0;
    pub const DELIVERY_MODE: u8 = 2;
    pub const ACK_FORMAT_FLAG: u8 = 4;
    pub const SECURITY_FLAG: u8 = 5;
    pub const ACK_FLAG: u8 = 6;
    pub const EXTENDED_HEADER_FLAG: u8 = 7;
}

mod mask {
    pub const FRAME_TYPE: u8 = 0x03;
    pub const DELIVERY_MODE: u8 = 0x0C;
    pub const ACK_FORMAT_FLAG: u8 = 0x10;
    pub const SECURITY_FLAG: u8 = 0x20;
    pub const ACK_FLAG: u8 = 0x40;
    pub const EXTENDED_HEADER_FLAG: u8 = 0x80;
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;

    #[test]
    fn parse_frame_control() {
        // bits 0-1: 01 (Command), bits 2-3: 00 (Unicast), rest: 0
        let raw = [0b0000_0001_u8];

        let (frame_control, len) = FrameControl::try_read(&raw, ()).unwrap();
        assert_eq!(len, 1);
        assert_eq!(frame_control.frame_type(), FrameType::Command);
        assert_eq!(frame_control.delivery_mode(), DeliveryMode::Unicast);
        assert!(!frame_control.ack_format_flag());
        assert!(!frame_control.security_flag());
        assert!(!frame_control.ack_request());
        assert!(!frame_control.extended_header());
    }

    #[test]
    fn parse_frame_control_data_broadcast_with_security() {
        // bits 0-1: 00 (Data), bits 2-3: 10 (Broadcast), bit 5: 1 (Security)
        let raw = [0b0010_1000_u8];

        let (frame_control, len) = FrameControl::try_read(&raw, ()).unwrap();
        assert_eq!(len, 1);
        assert_eq!(frame_control.frame_type(), FrameType::Data);
        assert_eq!(frame_control.delivery_mode(), DeliveryMode::Broadcast);
        assert!(!frame_control.ack_format_flag());
        assert!(frame_control.security_flag());
        assert!(!frame_control.ack_request());
        assert!(!frame_control.extended_header());
    }

    #[test]
    fn set_frame_control_roundtrip() {
        let fc = FrameControl::default()
            .set_frame_type(FrameType::Data)
            .set_delivery_mode(DeliveryMode::Broadcast);
        assert_eq!(fc.0, 0x08);
        assert_eq!(fc.frame_type(), FrameType::Data);
        assert_eq!(fc.delivery_mode(), DeliveryMode::Broadcast);
    }

    #[test]
    fn parse_extended_frame_control_with_fragmentation() {
        let raw = [0b01u8];

        let (frame_control, len) = ExtendedFrameControl::try_read(&raw, ()).unwrap();
        assert_eq!(len, 1);
        assert!(matches!(
            frame_control.fragmentation,
            Fragmentation::Fragmentation
        ));
    }

    #[test]
    fn parse_extended_frame_control_without_fragmentation() {
        let raw = [0b00u8];

        let (frame_control, len) = ExtendedFrameControl::try_read(&raw, ()).unwrap();
        assert_eq!(len, 1);
        assert!(matches!(
            frame_control.fragmentation,
            Fragmentation::NoFragmentation
        ));
    }
}
