use core::marker::PhantomData;

use super::error::ZclError;
use super::ids::TypeId;
use super::nullable::ZclHasNull;
use super::schema::ZclSchema;

/// Trait for types encodable as ZCL Enum8 (`TypeId` 0x30).
///
/// `from_raw` receives only non-sentinel values — the `Enum8` schema wrapper
/// rejects 0xFF (the null sentinel) before calling `from_raw`. `into_raw`
/// must not produce 0xFF; the wrapper rejects it on encode. Implement
/// `from_raw` to reject only semantically invalid enum values via
/// `Err(ZclError::InvalidEnumValue)`.
pub trait ZclEnum8: Sized + Copy {
    fn from_raw(raw: u8) -> Result<Self, ZclError>;
    fn into_raw(self) -> u8;
}

/// Trait for types encodable as ZCL Enum16 (`TypeId` 0x31).
///
/// `from_raw` receives only non-sentinel values — the `Enum16` schema wrapper
/// rejects 0xFFFF (the null sentinel) before calling `from_raw`. `into_raw`
/// must not produce 0xFFFF; the wrapper rejects it on encode. Implement
/// `from_raw` to reject only semantically invalid enum values via
/// `Err(ZclError::InvalidEnumValue)`.
pub trait ZclEnum16: Sized + Copy {
    fn from_raw(raw: u16) -> Result<Self, ZclError>;
    fn into_raw(self) -> u16;
}

/// Schema wrapper for `T: ZclEnum8`. Decoded value is bare `T`.
pub struct Enum8<T>(PhantomData<T>);

impl<T: ZclEnum8> ZclSchema for Enum8<T> {
    type Value<'a>
        = T
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Enum8;
    const ENCODED_SIZE: Option<usize> = Some(1);

    fn decode(bytes: &[u8]) -> Result<(T, usize), ZclError> {
        let raw = bytes.first().copied().ok_or(ZclError::InsufficientBytes)?;
        if raw == 0xFF {
            return Err(ZclError::NullSentinel);
        }
        let value = T::from_raw(raw)?;
        Ok((value, 1))
    }

    fn encode(value: T, bytes: &mut [u8]) -> Result<usize, ZclError> {
        let raw = value.into_raw();
        if raw == 0xFF {
            return Err(ZclError::NullSentinel);
        }
        bytes
            .first_mut()
            .map(|b| {
                *b = raw;
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl<T: ZclEnum8> ZclHasNull for Enum8<T> {
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

/// Schema wrapper for `T: ZclEnum16`. Decoded value is bare `T`.
pub struct Enum16<T>(PhantomData<T>);

impl<T: ZclEnum16> ZclSchema for Enum16<T> {
    type Value<'a>
        = T
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Enum16;
    const ENCODED_SIZE: Option<usize> = Some(2);

    fn decode(bytes: &[u8]) -> Result<(T, usize), ZclError> {
        let raw = bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| u16::from_le_bytes([s[0], s[1]]))?;
        if raw == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        let value = T::from_raw(raw)?;
        Ok((value, 2))
    }

    fn encode(value: T, bytes: &mut [u8]) -> Result<usize, ZclError> {
        let raw = value.into_raw();
        if raw == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&raw.to_le_bytes());
            2
        })
    }
}

impl<T: ZclEnum16> ZclHasNull for Enum16<T> {
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TestMode {
        Off = 0x00,
        On = 0x01,
        Auto = 0x02,
    }

    impl ZclEnum8 for TestMode {
        fn from_raw(raw: u8) -> Result<Self, ZclError> {
            match raw {
                0x00 => Ok(Self::Off),
                0x01 => Ok(Self::On),
                0x02 => Ok(Self::Auto),
                _ => Err(ZclError::InvalidEnumValue),
            }
        }
        fn into_raw(self) -> u8 {
            self as u8
        }
    }

    #[test]
    fn enum8_roundtrip() {
        let mut buf = [0u8; 1];
        assert_eq!(
            Enum8::<TestMode>::encode(TestMode::Auto, &mut buf).unwrap(),
            1
        );
        assert_eq!(buf, [0x02]);
        assert_eq!(
            Enum8::<TestMode>::decode(&buf).unwrap(),
            (TestMode::Auto, 1)
        );
    }

    #[test]
    fn enum8_decode_rejects_null_sentinel() {
        assert_eq!(
            Enum8::<TestMode>::decode(&[0xFF]).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn enum8_decode_rejects_invalid() {
        assert_eq!(
            Enum8::<TestMode>::decode(&[0x10]).unwrap_err(),
            ZclError::InvalidEnumValue
        );
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SentinelEnum8;

    impl ZclEnum8 for SentinelEnum8 {
        fn from_raw(_: u8) -> Result<Self, ZclError> {
            Ok(Self)
        }
        fn into_raw(self) -> u8 {
            0xFF
        }
    }

    #[test]
    fn enum8_encode_rejects_null_sentinel() {
        let mut buf = [0u8; 1];
        assert_eq!(
            Enum8::<SentinelEnum8>::encode(SentinelEnum8, &mut buf).unwrap_err(),
            ZclError::NullSentinel
        );
        assert_eq!(buf, [0]);
    }

    #[test]
    fn nullable_enum8_null() {
        assert_eq!(
            Nullable::<Enum8<TestMode>>::decode(&[0xFF]).unwrap(),
            (None, 1)
        );
    }

    #[test]
    fn nullable_enum8_value() {
        assert_eq!(
            Nullable::<Enum8<TestMode>>::decode(&[0x01]).unwrap(),
            (Some(TestMode::On), 1)
        );
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TestMode16 {
        Off = 0x0000,
        On = 0x0001,
    }

    impl ZclEnum16 for TestMode16 {
        fn from_raw(raw: u16) -> Result<Self, ZclError> {
            match raw {
                0x0000 => Ok(Self::Off),
                0x0001 => Ok(Self::On),
                _ => Err(ZclError::InvalidEnumValue),
            }
        }
        fn into_raw(self) -> u16 {
            self as u16
        }
    }

    #[test]
    fn enum16_roundtrip() {
        let mut buf = [0u8; 2];
        assert_eq!(
            Enum16::<TestMode16>::encode(TestMode16::On, &mut buf).unwrap(),
            2
        );
        assert_eq!(buf, [0x01, 0x00]);
        assert_eq!(
            Enum16::<TestMode16>::decode(&buf).unwrap(),
            (TestMode16::On, 2)
        );
    }

    #[test]
    fn enum16_decode_rejects_null_sentinel() {
        assert_eq!(
            Enum16::<TestMode16>::decode(&[0xFF, 0xFF]).unwrap_err(),
            ZclError::NullSentinel
        );
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SentinelEnum16;

    impl ZclEnum16 for SentinelEnum16 {
        fn from_raw(_: u16) -> Result<Self, ZclError> {
            Ok(Self)
        }
        fn into_raw(self) -> u16 {
            0xFFFF
        }
    }

    #[test]
    fn enum16_encode_rejects_null_sentinel() {
        let mut buf = [0u8; 2];
        assert_eq!(
            Enum16::<SentinelEnum16>::encode(SentinelEnum16, &mut buf).unwrap_err(),
            ZclError::NullSentinel
        );
        assert_eq!(buf, [0, 0]);
    }

    #[test]
    fn nullable_enum16_null() {
        assert_eq!(
            Nullable::<Enum16<TestMode16>>::decode(&[0xFF, 0xFF]).unwrap(),
            (None, 2)
        );
    }

    #[test]
    fn nullable_enum16_value() {
        assert_eq!(
            Nullable::<Enum16<TestMode16>>::decode(&[0x01, 0x00]).unwrap(),
            (Some(TestMode16::On), 2)
        );
    }
}
