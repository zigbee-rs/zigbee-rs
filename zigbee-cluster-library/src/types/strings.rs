use super::error::ZclError;
use super::ids::TypeId;
use super::nullable::ZclHasNull;
use super::schema::ZclSchema;

/// Borrowed character-string bytes with lazy UTF-8 validation.
/// Used for dynamic/untrusted frames where eager validation may be too
/// expensive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZclText<'a>(&'a [u8]);

impl<'a> ZclText<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.0
    }

    pub fn as_str(self) -> Result<&'a str, ZclError> {
        core::str::from_utf8(self.0).map_err(|_| ZclError::InvalidUtf8)
    }
}

/// Guaranteed-UTF-8 short string; max 254 bytes. Used on schema-known paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortStr<'a>(&'a str);

impl<'a> ShortStr<'a> {
    pub fn new(s: &'a str) -> Result<Self, ZclError> {
        if s.len() > 254 {
            return Err(ZclError::InvalidLength);
        }
        Ok(Self(s))
    }

    pub fn as_str(self) -> &'a str {
        self.0
    }
}

/// Guaranteed-UTF-8 long string; max 65534 bytes. Used on schema-known paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongStr<'a>(&'a str);

impl<'a> LongStr<'a> {
    pub fn new(s: &'a str) -> Result<Self, ZclError> {
        if s.len() > 65534 {
            return Err(ZclError::InvalidLength);
        }
        Ok(Self(s))
    }

    pub fn as_str(self) -> &'a str {
        self.0
    }
}

/// Schema for ZCL `CharacterString` (0x42). Value is `ShortStr<'_>`.
/// Length byte 0xFF is the null sentinel; bare `ShortText` rejects it.
/// Use `Nullable<ShortText>` for nullable attributes.
pub struct ShortText;

impl ZclSchema for ShortText {
    type Value<'a> = ShortStr<'a>;
    const TYPE_ID: TypeId = TypeId::CharacterString;
    const ENCODED_SIZE: Option<usize> = None;

    fn decode(bytes: &[u8]) -> Result<(ShortStr<'_>, usize), ZclError> {
        let len_byte = bytes.first().copied().ok_or(ZclError::InsufficientBytes)?;
        if len_byte == 0xFF {
            return Err(ZclError::NullSentinel);
        }
        let len = usize::from(len_byte);
        let raw = bytes.get(1..1 + len).ok_or(ZclError::InsufficientBytes)?;
        let s = core::str::from_utf8(raw).map_err(|_| ZclError::InvalidUtf8)?;
        Ok((ShortStr(s), 1 + len))
    }

    #[allow(clippy::cast_possible_truncation)]
    fn encode(value: ShortStr<'_>, bytes: &mut [u8]) -> Result<usize, ZclError> {
        let raw = value.0.as_bytes();
        let len = raw.len();
        if len > 254 {
            return Err(ZclError::InvalidLength);
        }
        let total = 1 + len;
        if bytes.len() < total {
            return Err(ZclError::BufferTooSmall);
        }
        bytes[0] = len as u8; // len ≤ 254, checked above
        bytes[1..total].copy_from_slice(raw);
        Ok(total)
    }
}

impl ZclHasNull for ShortText {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.first() == Some(&0xFF)).then_some(1)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.first_mut()
            .map(|b| {
                *b = 0xFF;
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

/// Schema for ZCL `LongCharacterString` (0x44). Value is `LongStr<'_>`.
pub struct LongText;

impl ZclSchema for LongText {
    type Value<'a> = LongStr<'a>;
    const TYPE_ID: TypeId = TypeId::LongCharacterString;
    const ENCODED_SIZE: Option<usize> = None;

    fn decode(bytes: &[u8]) -> Result<(LongStr<'_>, usize), ZclError> {
        if bytes.len() < 2 {
            return Err(ZclError::InsufficientBytes);
        }
        let len_u16 = u16::from_le_bytes([bytes[0], bytes[1]]);
        if len_u16 == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        let len = usize::from(len_u16);
        if len > 65534 {
            return Err(ZclError::InvalidLength);
        }
        let raw = bytes.get(2..2 + len).ok_or(ZclError::InsufficientBytes)?;
        let s = core::str::from_utf8(raw).map_err(|_| ZclError::InvalidUtf8)?;
        Ok((LongStr(s), 2 + len))
    }

    #[allow(clippy::cast_possible_truncation)]
    fn encode(value: LongStr<'_>, bytes: &mut [u8]) -> Result<usize, ZclError> {
        let raw = value.0.as_bytes();
        let len = raw.len();
        if len > 65534 {
            return Err(ZclError::InvalidLength);
        }
        let total = 2 + len;
        if bytes.len() < total {
            return Err(ZclError::BufferTooSmall);
        }
        bytes[..2].copy_from_slice(&(len as u16).to_le_bytes()); // len ≤ 65534, checked above
        bytes[2..total].copy_from_slice(raw);
        Ok(total)
    }
}

impl ZclHasNull for LongText {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..2) == Some(&[0xFF, 0xFF])).then_some(2)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..2)
            .map(|s| {
                s.copy_from_slice(&[0xFF, 0xFF]);
                2
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

/// Schema for ZCL `OctetString` (0x41). Value is `&[u8]`.
/// Length byte 0xFF is the null sentinel; bare `ShortOctetString` rejects it.
pub struct ShortOctetString;

impl ZclSchema for ShortOctetString {
    type Value<'a> = &'a [u8];
    const TYPE_ID: TypeId = TypeId::OctetString;
    const ENCODED_SIZE: Option<usize> = None;

    fn decode(bytes: &[u8]) -> Result<(&[u8], usize), ZclError> {
        let len_byte = bytes.first().copied().ok_or(ZclError::InsufficientBytes)?;
        if len_byte == 0xFF {
            return Err(ZclError::NullSentinel);
        }
        let len = usize::from(len_byte);
        let data = bytes.get(1..1 + len).ok_or(ZclError::InsufficientBytes)?;
        Ok((data, 1 + len))
    }

    #[allow(clippy::cast_possible_truncation)]
    fn encode(value: &[u8], bytes: &mut [u8]) -> Result<usize, ZclError> {
        let len = value.len();
        if len > 254 {
            return Err(ZclError::InvalidLength);
        }
        let total = 1 + len;
        if bytes.len() < total {
            return Err(ZclError::BufferTooSmall);
        }
        bytes[0] = len as u8; // len ≤ 254, checked above
        bytes[1..total].copy_from_slice(value);
        Ok(total)
    }
}

impl ZclHasNull for ShortOctetString {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.first() == Some(&0xFF)).then_some(1)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.first_mut()
            .map(|b| {
                *b = 0xFF;
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

/// Schema for ZCL `LongOctetString` (0x43). Value is `&[u8]`.
pub struct LongOctetString;

impl ZclSchema for LongOctetString {
    type Value<'a> = &'a [u8];
    const TYPE_ID: TypeId = TypeId::LongOctetString;
    const ENCODED_SIZE: Option<usize> = None;

    fn decode(bytes: &[u8]) -> Result<(&[u8], usize), ZclError> {
        if bytes.len() < 2 {
            return Err(ZclError::InsufficientBytes);
        }
        let len_u16 = u16::from_le_bytes([bytes[0], bytes[1]]);
        if len_u16 == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        let len = usize::from(len_u16);
        let data = bytes.get(2..2 + len).ok_or(ZclError::InsufficientBytes)?;
        Ok((data, 2 + len))
    }

    #[allow(clippy::cast_possible_truncation)]
    fn encode(value: &[u8], bytes: &mut [u8]) -> Result<usize, ZclError> {
        let len = value.len();
        if len > 65534 {
            return Err(ZclError::InvalidLength);
        }
        let total = 2 + len;
        if bytes.len() < total {
            return Err(ZclError::BufferTooSmall);
        }
        bytes[..2].copy_from_slice(&(len as u16).to_le_bytes()); // len ≤ 65534, checked above
        bytes[2..total].copy_from_slice(value);
        Ok(total)
    }
}

impl ZclHasNull for LongOctetString {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..2) == Some(&[0xFF, 0xFF])).then_some(2)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..2)
            .map(|s| {
                s.copy_from_slice(&[0xFF, 0xFF]);
                2
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::nullable::Nullable;

    #[test]
    fn short_str_new_max_len() {
        let s = "a".repeat(254);
        assert!(ShortStr::new(&s).is_ok());
    }

    #[test]
    fn short_str_new_rejects_over_254() {
        let s = "a".repeat(255);
        assert_eq!(ShortStr::new(&s).unwrap_err(), ZclError::InvalidLength);
    }

    #[test]
    fn short_text_roundtrip() {
        let mut buf = [0u8; 16];
        let s = ShortStr::new("Hello").unwrap();
        let n = ShortText::encode(s, &mut buf).unwrap();
        assert_eq!(n, 6);
        assert_eq!(&buf[..n], &[5, b'H', b'e', b'l', b'l', b'o']);
        let (decoded, used) = ShortText::decode(&buf[..n]).unwrap();
        assert_eq!(used, 6);
        assert_eq!(decoded.as_str(), "Hello");
    }

    #[test]
    fn short_text_decode_rejects_null_sentinel() {
        assert_eq!(
            ShortText::decode(&[0xFF]).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn nullable_short_text_null() {
        let (v, n) = Nullable::<ShortText>::decode(&[0xFF]).unwrap();
        assert_eq!(n, 1);
        assert!(v.is_none());
    }

    #[test]
    fn nullable_short_text_value() {
        let bytes = [5u8, b'H', b'e', b'l', b'l', b'o'];
        let (v, n) = Nullable::<ShortText>::decode(&bytes).unwrap();
        assert_eq!(n, 6);
        assert_eq!(v.unwrap().as_str(), "Hello");
    }

    #[test]
    fn short_octet_string_roundtrip() {
        let mut buf = [0u8; 8];
        let data: &[u8] = &[0xDE, 0xAD, 0xBE];
        let n = ShortOctetString::encode(data, &mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], &[3, 0xDE, 0xAD, 0xBE]);
        let (decoded, used) = ShortOctetString::decode(&buf[..n]).unwrap();
        assert_eq!(used, 4);
        assert_eq!(decoded, data);
    }

    #[test]
    fn long_text_roundtrip() {
        let mut buf = [0u8; 16];
        let s = LongStr::new("Hi").unwrap();
        let n = LongText::encode(s, &mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], &[2, 0, b'H', b'i']);
        let (decoded, used) = LongText::decode(&buf[..n]).unwrap();
        assert_eq!(used, 4);
        assert_eq!(decoded.as_str(), "Hi");
    }

    #[test]
    fn short_text_invalid_utf8_rejected() {
        let bytes = [2u8, 0xFF, 0xFE];
        assert_eq!(
            ShortText::decode(&bytes).unwrap_err(),
            ZclError::InvalidUtf8
        );
    }
}
