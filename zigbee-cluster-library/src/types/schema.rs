use super::error::ZclError;
use super::ids::AttributeId;
use super::ids::ClusterId;
use super::ids::TypeId;

/// Compile-time schema for one ZCL wire type. Associates a Rust value type with
/// a ZCL `TypeId`, fixed encoded size (if applicable), and byte-exact
/// encode/decode operations.
pub trait ZclSchema {
    type Value<'a>
    where
        Self: 'a;
    const TYPE_ID: TypeId;
    const ENCODED_SIZE: Option<usize> = None;
    /// True when every byte sequence of length `ENCODED_SIZE` decodes without
    /// error. Fixed-size collection validation skips per-element decode when
    /// true, relying on length alone. Must be false for types that reject some
    /// bit patterns — including `bool`, enums, and all numeric scalars (which
    /// reject their null-sentinel value).
    ///
    /// Must agree with `TypeId::all_patterns_valid()` for the corresponding
    /// `TYPE_ID`. There is no compile-time enforcement; keep them in sync when
    /// adding new schema impls.
    const ALL_PATTERNS_VALID: bool = false;
    fn decode(bytes: &[u8]) -> Result<(Self::Value<'_>, usize), ZclError>;
    fn decode_prefix(bytes: &[u8]) -> Result<(Self::Value<'_>, usize), ZclError> {
        Self::decode(bytes)
    }
    fn encode(value: Self::Value<'_>, bytes: &mut [u8]) -> Result<usize, ZclError>;
}

impl ZclSchema for bool {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Boolean;
    const ENCODED_SIZE: Option<usize> = Some(1);

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        match bytes.first().copied().ok_or(ZclError::InsufficientBytes)? {
            0x00 => Ok((false, 1)),
            0x01 => Ok((true, 1)),
            0xFF => Err(ZclError::NullSentinel),
            _ => Err(ZclError::InvalidValue),
        }
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes
            .first_mut()
            .map(|b| {
                *b = u8::from(value);
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclSchema for u8 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Uint8;
    const ENCODED_SIZE: Option<usize> = Some(1);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes.first().copied().ok_or(ZclError::InsufficientBytes)?;
        if value == 0xFF {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 1))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == 0xFF {
            return Err(ZclError::NullSentinel);
        }
        bytes
            .first_mut()
            .map(|b| {
                *b = value;
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclSchema for u16 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Uint16;
    const ENCODED_SIZE: Option<usize> = Some(2);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1]]))?;
        if value == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 2))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            2
        })
    }
}

impl ZclSchema for u32 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Uint32;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1], s[2], s[3]]))?;
        if value == 0xFFFF_FFFF {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == 0xFFFF_FFFF {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            4
        })
    }
}

impl ZclSchema for u64 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Uint64;
    const ENCODED_SIZE: Option<usize> = Some(8);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..8)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))?;
        if value == 0xFFFF_FFFF_FFFF_FFFF {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 8))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == 0xFFFF_FFFF_FFFF_FFFF {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..8).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            8
        })
    }
}

impl ZclSchema for i8 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Int8;
    const ENCODED_SIZE: Option<usize> = Some(1);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .first()
            .copied()
            .ok_or(ZclError::InsufficientBytes)
            .map(u8::cast_signed)?;
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 1))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        bytes
            .first_mut()
            .map(|b| {
                *b = value.cast_unsigned();
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclSchema for i16 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Int16;
    const ENCODED_SIZE: Option<usize> = Some(2);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1]]))?;
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 2))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            2
        })
    }
}

impl ZclSchema for i32 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Int32;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1], s[2], s[3]]))?;
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            4
        })
    }
}

impl ZclSchema for i64 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Int64;
    const ENCODED_SIZE: Option<usize> = Some(8);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..8)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))?;
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 8))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value == Self::MIN {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..8).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            8
        })
    }
}

/// Schema for IEEE 754 half-precision (semi-precision) float. Value is raw u16
/// bit pattern.
pub struct SemiPrecisionFloat;

impl ZclSchema for SemiPrecisionFloat {
    type Value<'a> = u16;
    const TYPE_ID: TypeId = TypeId::SemiPrecision;
    const ENCODED_SIZE: Option<usize> = Some(2);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(u16, usize), ZclError> {
        bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (u16::from_le_bytes([s[0], s[1]]), 2))
    }

    fn encode(value: u16, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            2
        })
    }
}

impl ZclSchema for f32 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::SinglePrecision;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1], s[2], s[3]]))?;
        if value.is_nan() {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value.is_nan() {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            4
        })
    }
}

impl ZclSchema for f64 {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::DoublePrecision;
    const ENCODED_SIZE: Option<usize> = Some(8);
    const ALL_PATTERNS_VALID: bool = false;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let value = bytes
            .get(..8)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| Self::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))?;
        if value.is_nan() {
            return Err(ZclError::NullSentinel);
        }
        Ok((value, 8))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        if value.is_nan() {
            return Err(ZclError::NullSentinel);
        }
        bytes.get_mut(..8).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.to_le_bytes());
            8
        })
    }
}

/// ZCL time-of-day (hours, minutes, seconds, hundredths packed into 4 bytes
/// LE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZclTimeOfDay(pub u32);

impl ZclSchema for ZclTimeOfDay {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::TimeOfDay;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (Self(u32::from_le_bytes([s[0], s[1], s[2], s[3]])), 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            4
        })
    }
}

/// ZCL date (year-1900, month, day-of-month, day-of-week packed into 4 bytes
/// LE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZclDate(pub u32);

impl ZclSchema for ZclDate {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::Date;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (Self(u32::from_le_bytes([s[0], s[1], s[2], s[3]])), 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            4
        })
    }
}

/// ZCL UTC time (seconds since 2000-01-01 00:00:00 UTC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcTime(pub u32);

impl ZclSchema for UtcTime {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::UtcTime;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (Self(u32::from_le_bytes([s[0], s[1], s[2], s[3]])), 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            4
        })
    }
}

/// ZCL IEEE 802.15.4 64-bit extended address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IeeeAddress(pub u64);

impl ZclSchema for IeeeAddress {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::IeeeAddress;
    const ENCODED_SIZE: Option<usize> = Some(8);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes.get(..8).ok_or(ZclError::InsufficientBytes).map(|s| {
            (
                Self(u64::from_le_bytes([
                    s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
                ])),
                8,
            )
        })
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..8).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            8
        })
    }
}

/// ZCL 128-bit network security key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecurityKey(pub [u8; 16]);

impl ZclSchema for SecurityKey {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::SecurityKey;
    const ENCODED_SIZE: Option<usize> = Some(16);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        let raw: &[u8; 16] = bytes
            .get(..16)
            .ok_or(ZclError::InsufficientBytes)?
            .try_into()
            .map_err(|_| ZclError::InsufficientBytes)?;
        Ok((Self(*raw), 16))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes
            .get_mut(..16)
            .ok_or(ZclError::BufferTooSmall)
            .map(|s| {
                s.copy_from_slice(&value.0);
                16
            })
    }
}

/// ZCL `BACnet` OID (object identifier).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BacnetOid(pub u32);

impl ZclSchema for BacnetOid {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::BacnetOid;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (Self(u32::from_le_bytes([s[0], s[1], s[2], s[3]])), 4))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            4
        })
    }
}

impl ZclSchema for ClusterId {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::ClusterId;
    const ENCODED_SIZE: Option<usize> = Some(2);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (Self(u16::from_le_bytes([s[0], s[1]])), 2))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            2
        })
    }
}

impl ZclSchema for AttributeId {
    type Value<'a> = Self;
    const TYPE_ID: TypeId = TypeId::AttributeId;
    const ENCODED_SIZE: Option<usize> = Some(2);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(Self, usize), ZclError> {
        bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| (Self(u16::from_le_bytes([s[0], s[1]])), 2))
    }

    fn encode(value: Self, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.0.to_le_bytes());
            2
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_roundtrip() {
        let mut buf = [0u8; 1];
        assert_eq!(bool::encode(true, &mut buf).unwrap(), 1);
        assert_eq!(buf, [0x01]);
        assert_eq!(bool::decode(&buf).unwrap(), (true, 1));

        assert_eq!(bool::encode(false, &mut buf).unwrap(), 1);
        assert_eq!(buf, [0x00]);
        assert_eq!(bool::decode(&buf).unwrap(), (false, 1));
    }

    #[test]
    fn bool_decode_rejects_invalid() {
        assert_eq!(bool::decode(&[0x02]).unwrap_err(), ZclError::InvalidValue);
        assert_eq!(bool::decode(&[0xFE]).unwrap_err(), ZclError::InvalidValue);
        assert_eq!(bool::decode(&[0xFF]).unwrap_err(), ZclError::NullSentinel);
    }

    #[test]
    fn u8_roundtrip() {
        let mut buf = [0u8; 1];
        assert_eq!(u8::encode(42, &mut buf).unwrap(), 1);
        assert_eq!(u8::decode(&buf).unwrap(), (42u8, 1));
    }

    #[test]
    fn u16_roundtrip() {
        let mut buf = [0u8; 2];
        assert_eq!(u16::encode(0x1234, &mut buf).unwrap(), 2);
        assert_eq!(buf, [0x34, 0x12]);
        assert_eq!(u16::decode(&buf).unwrap(), (0x1234u16, 2));
    }

    #[test]
    fn i16_roundtrip() {
        let mut buf = [0u8; 2];
        assert_eq!(i16::encode(-1, &mut buf).unwrap(), 2);
        assert_eq!(buf, [0xFF, 0xFF]);
        assert_eq!(i16::decode(&buf).unwrap(), (-1i16, 2));
    }

    #[test]
    fn u32_roundtrip() {
        let mut buf = [0u8; 4];
        assert_eq!(u32::encode(0xDEAD_BEEF, &mut buf).unwrap(), 4);
        assert_eq!(u32::decode(&buf).unwrap(), (0xDEAD_BEEFu32, 4));
    }

    #[test]
    fn u64_roundtrip() {
        let mut buf = [0u8; 8];
        assert_eq!(u64::encode(0xCAFE_BABE_DEAD_BEEF, &mut buf).unwrap(), 8);
        assert_eq!(u64::decode(&buf).unwrap(), (0xCAFE_BABE_DEAD_BEEFu64, 8));
    }

    #[test]
    fn f32_roundtrip() {
        let mut buf = [0u8; 4];
        assert_eq!(f32::encode(1.5, &mut buf).unwrap(), 4);
        assert_eq!(f32::decode(&buf).unwrap(), (1.5f32, 4));
    }

    #[test]
    fn scalar_schemas_reject_null_sentinels() {
        let mut buf = [0u8; 8];
        assert_eq!(u8::decode(&[0xFF]).unwrap_err(), ZclError::NullSentinel);
        assert_eq!(
            u8::encode(0xFF, &mut buf).unwrap_err(),
            ZclError::NullSentinel
        );
        assert_eq!(
            u16::decode(&[0xFF, 0xFF]).unwrap_err(),
            ZclError::NullSentinel
        );
        assert_eq!(
            i16::decode(&[0x00, 0x80]).unwrap_err(),
            ZclError::NullSentinel
        );
        assert_eq!(
            f32::decode(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap_err(),
            ZclError::NullSentinel
        );
        assert_eq!(
            f32::encode(f32::NAN, &mut buf).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn ieee_address_roundtrip() {
        let mut buf = [0u8; 8];
        let addr = IeeeAddress(0x0102_0304_0506_0708);
        assert_eq!(IeeeAddress::encode(addr, &mut buf).unwrap(), 8);
        assert_eq!(IeeeAddress::decode(&buf).unwrap(), (addr, 8));
    }

    #[test]
    fn security_key_roundtrip() {
        let mut buf = [0u8; 16];
        let key = SecurityKey([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(SecurityKey::encode(key, &mut buf).unwrap(), 16);
        assert_eq!(SecurityKey::decode(&buf).unwrap(), (key, 16));
    }

    #[test]
    fn utc_time_roundtrip() {
        let mut buf = [0u8; 4];
        let t = UtcTime(0x1234_5678);
        assert_eq!(UtcTime::encode(t, &mut buf).unwrap(), 4);
        assert_eq!(UtcTime::decode(&buf).unwrap(), (t, 4));
    }

    #[test]
    fn insufficient_bytes_errors() {
        assert_eq!(
            u16::decode(&[0x01]).unwrap_err(),
            ZclError::InsufficientBytes
        );
        assert_eq!(
            u32::decode(&[0x01, 0x02, 0x03]).unwrap_err(),
            ZclError::InsufficientBytes
        );
    }

    #[test]
    fn buffer_too_small_errors() {
        let mut buf = [0u8; 1];
        assert_eq!(
            u16::encode(1, &mut buf).unwrap_err(),
            ZclError::BufferTooSmall
        );
    }
}
