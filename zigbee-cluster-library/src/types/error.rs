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

impl From<byte::Error> for ZclError {
    fn from(error: byte::Error) -> Self {
        match error {
            byte::Error::Incomplete => Self::InsufficientBytes,
            byte::Error::BadInput { .. } | byte::Error::BadOffset(_) => Self::InvalidValue,
        }
    }
}

impl From<ZclError> for byte::Error {
    fn from(e: ZclError) -> Self {
        match e {
            ZclError::InsufficientBytes | ZclError::BufferTooSmall => Self::Incomplete,
            _ => bad_input!("invalid ZCL frame"),
        }
    }
}

/// Protocol-level attribute error. Variants map to ZCL status codes and become
/// per-record status entries in responses. Only `Codec` aborts dispatch.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttrError {
    UnsupportedAttribute,
    ReadOnly,
    WriteOnly,
    InvalidDataType,
    InvalidValue,
    Codec(ZclError),
}

impl AttrError {
    pub fn to_status(&self) -> Option<Status> {
        match self {
            Self::UnsupportedAttribute => Some(Status::UnsupportedAttribute),
            Self::ReadOnly => Some(Status::ReadOnly),
            Self::WriteOnly => Some(Status::NotAuthorized),
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

/// Maps an `AttrError` to a `ZclError` for codec-abort paths.
/// Non-`Codec` variants all have a ZCL status code and should have been
/// handled by the surrounding `to_status()` guard first; they are mapped
/// explicitly so new variants cause a compile error instead of silent fallback.
impl From<AttrError> for ZclError {
    fn from(e: AttrError) -> Self {
        match e {
            AttrError::Codec(ze) => ze,
            AttrError::UnsupportedAttribute
            | AttrError::ReadOnly
            | AttrError::WriteOnly
            | AttrError::InvalidDataType
            | AttrError::InvalidValue => Self::InvalidValue,
        }
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
            AttrError::WriteOnly.to_status(),
            Some(Status::NotAuthorized)
        );
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
