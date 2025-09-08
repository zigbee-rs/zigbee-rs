//! Security Frame Formats
use core::mem;

use crate::internal::macros::impl_byte;
use crate::internal::types::IeeeAddress;

impl_byte! {
    /// Auxiliary Frame Header Format
    ///
    /// See Section 4.5.1.
    #[derive(Debug, Clone, Copy)]
    pub struct AuxFrameHeader {
        /// Security control
        pub security_control: SecurityControl,
        /// Frame counter
        pub frame_counter: u32,
        /// Set only if [`SecurityControl::extended_nonce`] is `true`.
        #[parse_if = security_control.extended_nonce()]
        pub source_address: Option<IeeeAddress>,
        /// Set only if [`SecurityControl::key_identifier`] is `1`.
        #[parse_if = security_control.is_network_key()]
        pub key_sequence_number: Option<u8>,
    }
}

impl_byte! {
    /// Security Control
    ///
    /// See Section 4.5.1.1.
    #[derive(Clone, Copy, Default)]
    pub struct SecurityControl(pub u8);
}

impl core::fmt::Debug for SecurityControl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SecurityControl")
            .field("security_level", &self.security_level())
            .field("key_identifier", &self.key_identifier())
            .field("extended_nonce", &self.extended_nonce())
            .finish()
    }
}

impl SecurityControl {
    /// Indicates how a frame is secured.
    pub fn security_level(&self) -> SecurityLevel {
        SecurityLevel::from_bits(self.0 & mask::SECURITY_LEVEL)
    }

    pub fn set_security_level(&mut self, level: SecurityLevel) {
        self.0 = (self.0 & !mask::SECURITY_LEVEL) | ((level.into_bits()) << offset::SECURITY_LEVEL);
    }

    /// Identifies the key in use.
    pub fn key_identifier(&self) -> KeyIdentifier {
        // SAFETY: any 2 bit permutation is a valid KeyIdentifier
        unsafe { mem::transmute((self.0 >> 3) & 0b11) }
    }

    pub fn set_key_identifier(&mut self, key_id: KeyIdentifier) {
        self.0 = (self.0 & !mask::KEY_IDENTIFIER) | ((key_id as u8) << offset::KEY_IDENTIFIER);
    }

    pub(crate) fn is_network_key(&self) -> bool {
        self.key_identifier() == KeyIdentifier::Network
    }

    /// Set if the sender address of the auxiliary header is present.
    pub fn extended_nonce(&self) -> bool {
        self.0 >> 5 != 0
    }

    pub fn set_extended_nonce(&mut self, extended_nonce: bool) {
        self.0 =
            (self.0 & !mask::EXTENDED_NONCE) | ((extended_nonce as u8) << offset::EXTENDED_NONCE);
    }
}

mod mask {
    pub const SECURITY_LEVEL: u8 = 0b0000_0111;
    pub const KEY_IDENTIFIER: u8 = 0b0001_1000;
    pub const EXTENDED_NONCE: u8 = 0b0010_0000;
}

mod offset {
    pub const SECURITY_LEVEL: u8 = 0;
    pub const KEY_IDENTIFIER: u8 = 3;
    pub const EXTENDED_NONCE: u8 = 5;
}

impl_byte! {
    #[tag(u8)]
    /// Security Level
    ///
    /// See Section 4.5.1.1.1.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(missing_docs)]
    pub enum SecurityLevel {
        None = 0b000,
        Mic32 = 0b001,
        Mic64 = 0b010,
        Mic128 = 0b011,
        Enc = 0b100,
        EncMic32 = 0b101,
        EncMic64 = 0b110,
        EncMic128 = 0b111,
        #[fallback = true]
        Reserved(u8)
    }
}

impl SecurityLevel {
    pub fn mic_length(&self) -> usize {
        match self {
            Self::EncMic32 | Self::Mic32 => 4,
            Self::EncMic64 | Self::Mic64 => 8,
            Self::EncMic128 | Self::Mic128 => 16,
            Self::Reserved(_) | Self::None | Self::Enc => 0,
        }
    }

    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0b000 => Self::None,
            0b001 => Self::Mic32,
            0b010 => Self::Mic64,
            0b011 => Self::Mic128,
            0b100 => Self::Enc,
            0b101 => Self::EncMic32,
            0b110 => Self::EncMic64,
            0b111 => Self::EncMic128,
            _ => Self::Reserved(bits),
        }
    }

    pub fn into_bits(&self) -> u8 {
        match self {
            Self::None => 0b000,
            Self::Mic32 => 0b001,
            Self::Mic64 => 0b010,
            Self::Mic128 => 0b011,
            Self::Enc => 0b100,
            Self::EncMic32 => 0b101,
            Self::EncMic64 => 0b110,
            Self::EncMic128 => 0b111,
            Self::Reserved(_bits) => 0,
        }
    }
}

/// Key Identifier
///
/// See Section 4.5.1.1.2.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum KeyIdentifier {
    Data = 0b00,
    Network = 0b01,
    KeyTransport = 0b10,
    KeyLoad = 0b11,
}
