//! Security Service
//!
//! Security services provided for ZigBee include methods for key establishment, key transport, frame protection, and
//! device management.
use frame::AuxFrameHeader;
use frame::SecurityLevel;
use thiserror::Error;

pub mod frame;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("invalid key")]
    InvalidKey,
    #[error("invalid data")]
    InvalidData,
    #[error("parse error")]
    ParseError(byte::Error),
}

impl From<byte::Error> for SecurityError {
    fn from(value: byte::Error) -> Self {
        Self::ParseError(value)
    }
}

impl From<SecurityError> for byte::Error {
    fn from(value: SecurityError) -> Self {
        match value {
            SecurityError::InvalidKey => Self::BadInput {
                err: "security: invalid key",
            },
            SecurityError::InvalidData => Self::BadInput {
                err: "security: invalid data",
            },
            SecurityError::ParseError(e) => e,
        }
    }
}

pub struct SecurityContext {}

impl SecurityContext {
    pub fn no_security() -> Self {
        Self {}
    }

    pub fn secure_frame(&self) {}

    pub fn unsecure_frame<'a>(
        &self,
        aux_header: &AuxFrameHeader,
        bytes: &'a [u8],
        offset: &mut usize,
    ) -> Result<&'a [u8], SecurityError> {
        let mic_length = aux_header.security_control.security_level().mic_length();
        byte::check_len(bytes, mic_length)?;
        let len = bytes.len() - mic_length;

        // Sec 4.3.1.2: overwrite the security level with the value from the NIB
        // (default 0x05)
        let _security_level = SecurityLevel::EncMic32;

        // TODO: impl

        // read the whole frame but return only the payload w/o MIC
        *offset = bytes.len();
        Ok(&bytes[..len])
    }
}
