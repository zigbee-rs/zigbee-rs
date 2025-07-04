use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::LE;

use crate::internal::macros::impl_byte;

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
    /// Extended Header Sub-Frame
    ///
    /// See Section 2.2.5.1.8
    #[repr(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Fragmentation {
        NoFragmentation = 0b00,
        Fragmentation = 0b01,
        PartOfFragmentedTransmission = 0b10,
        #[fallback = true]
        Reserved,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;

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
