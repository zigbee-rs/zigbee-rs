use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::LE;

use crate::internal::macros::impl_byte;

impl_byte! {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ExtendedFrameControlField {
        pub extended_frame_control: ExtendedFrameControl,
        pub block_number: u8,
        pub ack_bitfield: u8,
    }
}

impl_byte! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ExtendedFrameControl {
        fragmentation: Fragmentation
    }
}

/// Extended Header Sub-Frame
///
/// See Section 2.2.5.1.8
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fragmentation {
    NoFragmentation = 0b00,
    Fragmentation = 0b01,
    PartOfFragmentedTransmission = 0b10,
    Reserved = 0b11,
}

impl TryRead<'_, byte::ctx::Endian> for Fragmentation {
    fn try_read(bytes: &'_ [u8], _: byte::ctx::Endian) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let id: u8 = bytes.read_with(offset, LE)?;
        let command = match id {
            0b00 => Self::NoFragmentation,
            0b01 => Self::Fragmentation,
            0b10 => Self::PartOfFragmentedTransmission,
            _ => Self::Reserved,
        };

        Ok((command, *offset))
    }
}

impl TryWrite<byte::ctx::Endian> for Fragmentation {
    fn try_write(self, bytes: &mut [u8], _: byte::ctx::Endian) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self as u8)?;
        Ok(*offset)
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
