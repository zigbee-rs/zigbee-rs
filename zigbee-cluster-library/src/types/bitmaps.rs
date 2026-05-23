use core::marker::PhantomData;

use super::error::ZclError;
use super::ids::TypeId;
use super::schema::ZclSchema;

/// Trait for types encodable as ZCL Bitmap8. All u8 bit patterns are valid —
/// decode is infallible.
pub trait ZclBitmap8: Sized + Copy {
    fn from_bits(bits: u8) -> Self;
    fn into_bits(self) -> u8;
}

/// Trait for types encodable as ZCL Bitmap16. All u16 bit patterns are valid.
pub trait ZclBitmap16: Sized + Copy {
    fn from_bits(bits: u16) -> Self;
    fn into_bits(self) -> u16;
}

/// Trait for types encodable as ZCL Bitmap32. All u32 bit patterns are valid.
pub trait ZclBitmap32: Sized + Copy {
    fn from_bits(bits: u32) -> Self;
    fn into_bits(self) -> u32;
}

/// Trait for types encodable as ZCL Bitmap64. All u64 bit patterns are valid.
pub trait ZclBitmap64: Sized + Copy {
    fn from_bits(bits: u64) -> Self;
    fn into_bits(self) -> u64;
}

/// Schema wrapper for `T: ZclBitmap8`. Decode is infallible — all bit patterns
/// valid. Does NOT implement `ZclHasNull` — bitmaps have no null sentinel.
pub struct Bitmap8<T>(PhantomData<T>);

impl<T: ZclBitmap8> ZclSchema for Bitmap8<T> {
    type Value<'a>
        = T
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Bitmap8;
    const ENCODED_SIZE: Option<usize> = Some(1);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(T, usize), ZclError> {
        let raw = bytes.first().copied().ok_or(ZclError::InsufficientBytes)?;
        Ok((T::from_bits(raw), 1))
    }

    fn encode(value: T, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes
            .first_mut()
            .map(|b| {
                *b = value.into_bits();
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

/// Schema wrapper for `T: ZclBitmap16`. Decode is infallible.
pub struct Bitmap16<T>(PhantomData<T>);

impl<T: ZclBitmap16> ZclSchema for Bitmap16<T> {
    type Value<'a>
        = T
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Bitmap16;
    const ENCODED_SIZE: Option<usize> = Some(2);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(T, usize), ZclError> {
        let raw = bytes
            .get(..2)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| u16::from_le_bytes([s[0], s[1]]))?;
        Ok((T::from_bits(raw), 2))
    }

    fn encode(value: T, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..2).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.into_bits().to_le_bytes());
            2
        })
    }
}

/// Schema wrapper for `T: ZclBitmap32`. Decode is infallible.
pub struct Bitmap32<T>(PhantomData<T>);

impl<T: ZclBitmap32> ZclSchema for Bitmap32<T> {
    type Value<'a>
        = T
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Bitmap32;
    const ENCODED_SIZE: Option<usize> = Some(4);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(T, usize), ZclError> {
        let raw = bytes
            .get(..4)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))?;
        Ok((T::from_bits(raw), 4))
    }

    fn encode(value: T, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..4).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.into_bits().to_le_bytes());
            4
        })
    }
}

/// Schema wrapper for `T: ZclBitmap64`. Decode is infallible.
pub struct Bitmap64<T>(PhantomData<T>);

impl<T: ZclBitmap64> ZclSchema for Bitmap64<T> {
    type Value<'a>
        = T
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Bitmap64;
    const ENCODED_SIZE: Option<usize> = Some(8);
    const ALL_PATTERNS_VALID: bool = true;

    fn decode(bytes: &[u8]) -> Result<(T, usize), ZclError> {
        let raw = bytes
            .get(..8)
            .ok_or(ZclError::InsufficientBytes)
            .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))?;
        Ok((T::from_bits(raw), 8))
    }

    fn encode(value: T, bytes: &mut [u8]) -> Result<usize, ZclError> {
        bytes.get_mut(..8).ok_or(ZclError::BufferTooSmall).map(|s| {
            s.copy_from_slice(&value.into_bits().to_le_bytes());
            8
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestFlags(u8);

    impl ZclBitmap8 for TestFlags {
        fn from_bits(bits: u8) -> Self {
            Self(bits)
        }
        fn into_bits(self) -> u8 {
            self.0
        }
    }

    #[test]
    fn bitmap8_roundtrip() {
        let mut buf = [0u8; 1];
        assert_eq!(
            Bitmap8::<TestFlags>::encode(TestFlags(0b1010_1010), &mut buf).unwrap(),
            1
        );
        assert_eq!(buf, [0b1010_1010]);
        assert_eq!(
            Bitmap8::<TestFlags>::decode(&buf).unwrap(),
            (TestFlags(0b1010_1010), 1)
        );
    }

    #[test]
    fn bitmap8_accepts_all_bit_patterns() {
        // 0xFF is not a null sentinel for bitmaps — must decode successfully
        assert_eq!(
            Bitmap8::<TestFlags>::decode(&[0xFF]).unwrap(),
            (TestFlags(0xFF), 1)
        );
        assert_eq!(
            Bitmap8::<TestFlags>::decode(&[0x00]).unwrap(),
            (TestFlags(0x00), 1)
        );
    }
}
