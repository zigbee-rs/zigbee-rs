use super::ids::TypeId;
use crate::frame::Status;

/// Codec-only error. Every variant aborts dispatch; caller must not send the
/// response buffer.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ZclError {
    InsufficientBytes,
    BufferTooSmall,
    TypeIdMismatch {
        expected: TypeId,
        found: TypeId,
    },
    NullSentinel,
    InvalidEnumValue,
    InvalidValue,
    InvalidUtf8,
    InvalidLength,
    /// A `finish()` call on a decoder (struct or collection) found unconsumed
    /// fields or elements — the wire count did not match the number decoded.
    UnconsumedData,
}

/// Protocol-level attribute error. Variants map to ZCL status codes and become
/// per-record status entries in responses. Only `Codec` aborts dispatch.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttrError {
    UnsupportedAttribute,
    ReadOnly,
    InvalidDataType,
    InvalidValue,
    Codec(ZclError),
}

impl AttrError {
    pub fn to_status(&self) -> Option<Status> {
        match self {
            Self::UnsupportedAttribute => Some(Status::UnsupportedAttribute),
            Self::ReadOnly => Some(Status::ReadOnly),
            Self::InvalidDataType => Some(Status::InvalidDataType),
            Self::InvalidValue => Some(Status::InvalidValue),
            Self::Codec(_) => None,
        }
    }
}

impl From<ZclError> for AttrError {
    fn from(e: ZclError) -> Self {
        Self::Codec(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_error_maps_to_public_frame_status() {
        assert_eq!(
            AttrError::UnsupportedAttribute.to_status(),
            Some(Status::UnsupportedAttribute)
        );
        assert_eq!(AttrError::ReadOnly.to_status(), Some(Status::ReadOnly));
        assert_eq!(
            AttrError::InvalidDataType.to_status(),
            Some(Status::InvalidDataType)
        );
        assert_eq!(
            AttrError::InvalidValue.to_status(),
            Some(Status::InvalidValue)
        );
        assert_eq!(AttrError::Codec(ZclError::InvalidLength).to_status(), None);
    }
}
