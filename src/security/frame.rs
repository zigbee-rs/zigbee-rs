//! Security Frame Formats
use core::mem;

use crate::common::types::IeeeAddress;
use crate::impl_pack_bytes;

impl_pack_bytes! {
    /// Auxiliary Frame Header Format
    ///
    /// See Section 4.5.1.
    pub struct AuxFrameHeader {
        /// Security control
        #[control_header = SecurityControl]
        pub security_control: SecurityControl,
        /// Frame counter
        #[pack = true]
        pub frame_counter: u32,
        /// Set only if [`SecurityControl::extended_nonce`] is `true`.
        #[pack_if = SecurityControl::extended_nonce]
        pub source_address: Option<IeeeAddress>,
        /// Set only if [`SecurityControl::key_identifier`] is `1`.
        #[pack_if = SecurityControl::is_network_key]
        pub key_sequence_numner: Option<u8>,
    }
}

impl_pack_bytes! {
    /// Security Control
    ///
    /// See Section 4.5.1.1.
    #[derive(Clone, Copy)]
    pub struct SecurityControl(pub u8);
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
