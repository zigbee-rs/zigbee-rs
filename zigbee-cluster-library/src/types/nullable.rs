//! Nullable attributes.
//!
//! See ZCL Section 2.6.2.
//!
//! Most ZCL types reserve one bit pattern as a non-value, meaning "unknown" or
//! "not applicable". [`ZclKind`] impls reject it, so a value that decodes is
//! always meaningful; wrap a type in [`Nullable`] to accept it as `None`
//! instead.

use core::marker::PhantomData;

use super::codec::ZclKind;
use super::ids::TypeId;

/// A ZCL type with a defined non-value on the wire.
///
/// Bitmaps do not implement this — every bit pattern of a bitmap is a value —
/// so `Nullable<Bitmap8<T>>` does not compile.
pub trait ZclHasNull: ZclKind {
    /// Width of the non-value if `bytes` carries it at `offset`, else `None`.
    fn null_size(bytes: &[u8], offset: usize) -> Option<usize>;

    /// Write the non-value, advancing `offset`.
    fn write_null(bytes: &mut [u8], offset: &mut usize) -> byte::Result<()>;
}

/// Accepts the wrapped type's non-value as `None` (2.6.2).
///
/// Nullability is semantic rather than a wire distinction, so the type
/// identifier is the wrapped type's.
pub struct Nullable<T>(PhantomData<T>);

impl<T: ZclHasNull> ZclKind for Nullable<T> {
    type Value<'a> = Option<T::Value<'a>>;
    const TYPE_ID: TypeId = T::TYPE_ID;
    const ENCODED_SIZE: Option<usize> = T::ENCODED_SIZE;

    #[allow(single_use_lifetimes)]
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<Option<T::Value<'a>>> {
        if let Some(width) = T::null_size(bytes, *offset) {
            *offset += width;
            return Ok(None);
        }
        T::read(bytes, offset).map(Some)
    }

    fn write(
        value: Option<T::Value<'_>>,
        bytes: &mut [u8],
        offset: &mut usize,
    ) -> byte::Result<()> {
        match value {
            None => T::write_null(bytes, offset),
            Some(value) => {
                let start = *offset;
                T::write(value, bytes, offset)?;
                // a value that encodes to the non-value would read back as
                // None, so reject it rather than emit an ambiguous record
                if T::null_size(bytes, start).is_some() {
                    return Err(bad_input!("value encodes to the non-value"));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::integers::Uint8;

    #[test]
    fn the_non_value_reads_as_none_and_round_trips() {
        let mut buf = [0u8; 4];
        let offset = &mut 0;
        Nullable::<Uint8>::write(None, &mut buf, offset).expect("encoded");
        assert_eq!(&buf[..*offset], &[0xFF]);
        assert_eq!(
            Nullable::<Uint8>::read(&buf, &mut 0).expect("decoded"),
            None
        );
    }

    #[test]
    fn a_value_reads_as_some() {
        assert_eq!(
            Nullable::<Uint8>::read(&[0x2a], &mut 0).expect("decoded"),
            Some(Uint8(0x2a))
        );
    }

    // nullability is semantic: the wire type is unchanged
    #[test]
    fn the_type_identifier_is_the_wrapped_one() {
        assert_eq!(
            <Nullable<Uint8> as ZclKind>::TYPE_ID,
            <Uint8 as ZclKind>::TYPE_ID
        );
    }
}
