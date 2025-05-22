use byte::{BytesExt, TryRead, TryWrite};

use crate::impl_byte;

impl_byte! {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) struct ExtendedFrameControlField {
        pub extended_frame_control: ExtendedFrameControl,
        pub block_number: BlockNumber,
        pub ack_bitfield: AckBitfield,
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

impl TryRead<'_, ()> for Fragmentation {
    fn try_read(bytes: &'_ [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let id: u8 = bytes.read_with(offset, ())?;
        let command = match id {
            0b00 => Self::NoFragmentation,
            0b01 => Self::Fragmentation,
            0b10 => Self::PartOfFragmentedTransmission,
            _ => Self::Reserved,
        };

        Ok((command, *offset))
    }
}

impl TryWrite for Fragmentation {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self as u8)?;
        Ok(*offset)
    }
}



impl_byte! {
    /// Block number
    ///
    /// See Section 2.2.5.1.8
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct BlockNumber(pub u8);
}

impl_byte! {
    /// Ack Bitfield
    ///
    /// See Section 2.2.5.1.8
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct AckBitfield(pub u8);
}
