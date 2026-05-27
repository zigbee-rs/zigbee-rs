use core::marker::PhantomData;

use super::error::ZclError;
use super::schema::ZclSchema;

/// Marker trait for ZCL types that have a defined null/invalid sentinel on the
/// wire. Types that implement this can be wrapped in `Nullable<T>`.
/// Bitmap types do NOT implement this — all bit patterns are valid for bitmaps.
pub trait ZclHasNull: ZclSchema {
    /// If `bytes` begins with the null sentinel for this type, return
    /// `Some(bytes_consumed)`. Returns `None` if the bytes do not begin
    /// with the null sentinel.
    fn null_size(bytes: &[u8]) -> Option<usize>;
    /// Write the null sentinel into `buf`. Returns bytes written.
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError>;
}

/// Schema wrapper for types with a null sentinel. Decoded value is
/// `Option<T::Value<'_>>`. `TYPE_ID` is the same as the wrapped type —
/// nullability is semantic, not a wire distinction.
///
/// `Nullable<Bitmap8<T>>` does not compile because `Bitmap8<T>` does not
/// implement `ZclHasNull`.
pub struct Nullable<T>(PhantomData<T>);

impl<T: ZclHasNull> ZclSchema for Nullable<T> {
    type Value<'a>
        = Option<T::Value<'a>>
    where
        T: 'a;
    const TYPE_ID: super::ids::TypeId = T::TYPE_ID;
    const ENCODED_SIZE: Option<usize> = T::ENCODED_SIZE;

    fn decode(bytes: &[u8]) -> Result<(Option<T::Value<'_>>, usize), ZclError> {
        if let Some(n) = T::null_size(bytes) {
            return Ok((None, n));
        }
        let (value, n) = T::decode(bytes)?;
        Ok((Some(value), n))
    }

    fn decode_prefix(bytes: &[u8]) -> Result<(Option<T::Value<'_>>, usize), ZclError> {
        if let Some(n) = T::null_size(bytes) {
            return Ok((None, n));
        }
        let (value, n) = T::decode_prefix(bytes)?;
        Ok((Some(value), n))
    }

    fn encode(value: Option<T::Value<'_>>, bytes: &mut [u8]) -> Result<usize, ZclError> {
        match value {
            None => T::encode_null(bytes),
            Some(v) => {
                let written = T::encode(v, bytes)?;
                // redundant for built in types, added for safety if other ZclHasNull
                // impls encode a sentinel value
                if T::null_size(&bytes[..written]).is_some() {
                    return Err(ZclError::InvalidValue);
                }
                Ok(written)
            }
        }
    }
}

impl ZclHasNull for bool {
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

impl ZclHasNull for u8 {
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

impl ZclHasNull for u16 {
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

impl ZclHasNull for u32 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..4) == Some(&[0xFF, 0xFF, 0xFF, 0xFF])).then_some(4)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..4)
            .map(|s| {
                s.copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
                4
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for u64 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..8) == Some(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])).then_some(8)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..8)
            .map(|s| {
                s.copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
                8
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for i8 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.first() == Some(&0x80)).then_some(1)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.first_mut()
            .map(|b| {
                *b = 0x80;
                1
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for i16 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..2) == Some(&[0x00, 0x80])).then_some(2)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..2)
            .map(|s| {
                s.copy_from_slice(&[0x00, 0x80]);
                2
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for i32 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..4) == Some(&[0x00, 0x00, 0x00, 0x80])).then_some(4)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..4)
            .map(|s| {
                s.copy_from_slice(&[0x00, 0x00, 0x00, 0x80]);
                4
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for i64 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.get(..8) == Some(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80])).then_some(8)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        buf.get_mut(..8)
            .map(|s| {
                s.copy_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]);
                8
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for f32 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        let arr = bytes.get(..4)?;
        let v = Self::from_le_bytes([arr[0], arr[1], arr[2], arr[3]]);
        v.is_nan().then_some(4)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        // Canonical quiet NaN (0x7FC00000 LE) — matches zigpy and common ZCL stacks.
        // Any NaN decodes as null; we encode the canonical form for interop.
        buf.get_mut(..4)
            .map(|s| {
                s.copy_from_slice(&[0x00, 0x00, 0xC0, 0x7F]);
                4
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

impl ZclHasNull for f64 {
    fn null_size(bytes: &[u8]) -> Option<usize> {
        let arr = bytes.get(..8)?;
        let v = Self::from_le_bytes([
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ]);
        v.is_nan().then_some(8)
    }
    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        // Quiet NaN (0x7FF8000000000000 LE), matches zigpy.
        buf.get_mut(..8)
            .map(|s| {
                s.copy_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF8, 0x7F]);
                8
            })
            .ok_or(ZclError::BufferTooSmall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullable_u8_null() {
        assert_eq!(Nullable::<u8>::decode(&[0xFF]).unwrap(), (None, 1));
    }

    #[test]
    fn nullable_u8_value() {
        assert_eq!(Nullable::<u8>::decode(&[0x00]).unwrap(), (Some(0u8), 1));
        assert_eq!(Nullable::<u8>::decode(&[0x2A]).unwrap(), (Some(42u8), 1));
    }

    #[test]
    fn nullable_u16_null() {
        assert_eq!(Nullable::<u16>::decode(&[0xFF, 0xFF]).unwrap(), (None, 2));
    }

    #[test]
    fn nullable_u16_value() {
        assert_eq!(
            Nullable::<u16>::decode(&[0x34, 0x12]).unwrap(),
            (Some(0x1234u16), 2)
        );
    }

    #[test]
    fn nullable_i16_null() {
        assert_eq!(Nullable::<i16>::decode(&[0x00, 0x80]).unwrap(), (None, 2));
    }

    #[test]
    fn nullable_i16_value() {
        assert_eq!(
            Nullable::<i16>::decode(&[0xFF, 0xFF]).unwrap(),
            (Some(-1i16), 2)
        );
    }

    #[test]
    fn nullable_bool_null() {
        assert_eq!(Nullable::<bool>::decode(&[0xFF]).unwrap(), (None, 1));
    }

    #[test]
    fn nullable_bool_value() {
        assert_eq!(Nullable::<bool>::decode(&[0x01]).unwrap(), (Some(true), 1));
        assert_eq!(Nullable::<bool>::decode(&[0x00]).unwrap(), (Some(false), 1));
    }

    #[test]
    fn nullable_i32_null() {
        assert_eq!(
            Nullable::<i32>::decode(&[0x00, 0x00, 0x00, 0x80]).unwrap(),
            (None, 4)
        );
    }

    #[test]
    fn nullable_encode_null() {
        let mut buf = [0u8; 2];
        assert_eq!(Nullable::<i16>::encode(None, &mut buf).unwrap(), 2);
        assert_eq!(buf, [0x00, 0x80]);
    }

    #[test]
    fn nullable_encode_value() {
        let mut buf = [0u8; 2];
        assert_eq!(Nullable::<i16>::encode(Some(-100), &mut buf).unwrap(), 2);
        assert_eq!(buf, (-100i16).to_le_bytes());
    }
}
