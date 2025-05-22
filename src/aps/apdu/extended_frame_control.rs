
#[derive(Debug)]
struct ExtendedFrameControlField {
    pub extended_frame_control: ExtendedFrameControl,
    pub block_number: BlockNumber,
    pub ack_bitfield: AckBitfield,

}

#[derive(Debug)]
struct ExtendedFrameControl {
    fragmentation: Fragmentation
}

/// Extended Header Sub-Frame
///
/// See Section 2.2.5.1.8
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fragmentation {
    NoFragmentation = 0b00,
    Fragmntation = 0b01,
    PartOfFragmentedTransmission = 0b10,
    Reserved = 0b11,
}

/// Block number
///
/// See Section 2.2.5.1.8
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BlockNumber(pub u8);


/// Ack Bitfield
///
/// See Section 2.2.5.1.8
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AckBitfield(pub u8);

