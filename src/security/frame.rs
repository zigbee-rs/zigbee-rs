//! Security Frame Formats
use core::mem;

use crate::common::types::IeeeAddress;
use crate::impl_byte;

impl_byte! {
    /// Auxiliary Frame Header Format
    ///
    /// See Section 4.5.1.
    #[derive(Debug)]
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
        pub key_sequence_numner: Option<u8>,
    }
}

impl_byte! {
    /// Security Control
    ///
    /// See Section 4.5.1.1.
    #[derive(Clone, Copy)]
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
        // SAFETY: any 3 bit permutation is a valid SecurityLevel
        unsafe { mem::transmute(self.0 & 0b111) }
    }

    /// Identifies the key in use.
    pub fn key_identifier(&self) -> KeyIdentifier {
        // SAFETY: any 2 bit permutation is a valid KeyIdentifier
        unsafe { mem::transmute((self.0 >> 3) & 0b11) }
    }

    pub(crate) fn is_network_key(&self) -> bool {
        self.key_identifier() == KeyIdentifier::Network
    }

    /// Set if the sender address of the auxiliary header is present.
    pub fn extended_nonce(&self) -> bool {
        self.0 >> 5 != 0
    }
}

/// Security Level
///
/// See Section 4.5.1.1.1.
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SecurityLevel {
    #[default]
    None = 0b000,
    Mic32 = 0b001,
    Mic64 = 0b010,
    Mic128 = 0b011,
    Enc = 0b100,
    EncMic32 = 0b101,
    EncMic64 = 0b110,
    EncMic128 = 0b111,
}

impl SecurityLevel {
    pub fn mic_length(&self) -> usize {
        match self {
            Self::EncMic32 | Self::Mic32 => 4,
            Self::EncMic64 | Self::Mic64 => 8,
            Self::EncMic128 | Self::Mic128 => 16,
            Self::None | Self::Enc => 0,
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
