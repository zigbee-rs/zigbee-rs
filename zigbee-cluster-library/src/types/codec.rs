//! Attribute codec.
//!
//! See ZCL Section 2.6.2.
//!
//! [`ZclKind`] names one ZCL wire type: its type identifier and its byte-exact
//! codec. It is implemented on a lifetime-free *kind* rather than on the
//! decoded value, for two reasons: one Rust type can then appear under several
//! wire types — `Bitmap8<Flags>` and `Enum8<Flags>` coexist — and attribute
//! descriptors, which are `const`, can name a kind whose value borrows from
//! the frame.

use byte::BytesExt;
use byte::ctx;

use super::ids::TypeId;

/// One ZCL wire type (2.6.2).
///
/// Values are read and written through [`byte`], so a ZCL type composes with
/// the rest of the stack's serialization; `Self::Value` is what a frame
/// decodes to.
pub trait ZclKind {
    /// Value this type decodes to, borrowing from the frame.
    type Value<'a>;

    /// Type identifier written ahead of the value.
    const TYPE_ID: TypeId;

    /// Encoded width, or `None` for variable-length types.
    const ENCODED_SIZE: Option<usize> = None;

    /// Whether every byte pattern of `ENCODED_SIZE` decodes. `false` for types
    /// rejecting some patterns, including booleans, enumerations and any
    /// numeric type rejecting its non-value.
    ///
    /// A fixed-width collection validates by length alone when this holds,
    /// skipping the per-element decode.
    const ALL_PATTERNS_VALID: bool = false;

    /// Read a value from `bytes` at `offset`, advancing it past the value.
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<Self::Value<'a>>;

    /// Write `value` at `offset`, advancing it past what was written.
    fn write(value: Self::Value<'_>, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()>;

    /// Read a value whose type identifier has already been parsed, rejecting
    /// one that is not this type's.
    fn read_typed<'a>(
        type_id: TypeId,
        bytes: &'a [u8],
        offset: &mut usize,
    ) -> byte::Result<Self::Value<'a>> {
        if type_id != Self::TYPE_ID {
            return Err(bad_input!("attribute data type mismatch"));
        }
        Self::read(bytes, offset)
    }

    /// Write `type | value`, the pair carried by an attribute record (2.5.2).
    fn write_typed(
        value: Self::Value<'_>,
        bytes: &mut [u8],
        offset: &mut usize,
    ) -> byte::Result<()> {
        bytes.write_with(offset, Self::TYPE_ID.as_u8(), ctx::LE)?;
        Self::write(value, bytes, offset)
    }
}

/// Declares a ZCL data type whose value is the type itself: the `byte` codec
/// plus its [`ZclKind`] impl.
///
/// `#[width]` selects the odd-width integer codec for the types that have no
/// Rust primitive (24, 40, 48 and 56 bit); without it the inner type's own
/// little-endian codec is used.
macro_rules! zcl_type {
    (
        #[type_id = $type_id:expr]
        $(#[width = $width:literal])?
        #[null = $null:expr]
        $(#[$m:meta])*
        $v:vis struct $name:ident($vt:vis $ty:ty);
    ) => {
        ::zigbee_macros::impl_byte! {
            $(#[width = $width])?
            $(#[$m])*
            $v struct $name($vt $ty);
        }

        impl $crate::types::codec::ZclKind for $name {
            type Value<'a> = Self;
            const TYPE_ID: $crate::types::ids::TypeId = $type_id;
            const ENCODED_SIZE: Option<usize> = Some($crate::types::codec::zcl_type!(
                @size $ty $(, $width)?
            ));
            fn read(bytes: &[u8], offset: &mut usize) -> ::byte::Result<Self> {
                let value: Self = ::byte::BytesExt::read_with(bytes, offset, ())?;
                if value.0 == $null {
                    return Err(bad_input!("numeric non-value"));
                }
                Ok(value)
            }

            fn write(value: Self, bytes: &mut [u8], offset: &mut usize) -> ::byte::Result<()> {
                if value.0 == $null {
                    return Err(bad_input!("numeric non-value"));
                }
                ::byte::BytesExt::write_with(bytes, offset, value, ())?;
                Ok(())
            }
        }

        impl $crate::types::nullable::ZclHasNull for $name {
            fn null_size(bytes: &[u8], offset: usize) -> Option<usize> {
                let width = <Self as $crate::types::codec::ZclKind>::ENCODED_SIZE?;
                let mut scratch = [0u8; 8];
                let at = &mut 0;
                ::byte::BytesExt::write_with(&mut scratch[..], at, $name($null), ()).ok()?;
                (bytes.get(offset..offset + width)? == &scratch[..width]).then_some(width)
            }

            fn write_null(bytes: &mut [u8], offset: &mut usize) -> ::byte::Result<()> {
                ::byte::BytesExt::write_with(bytes, offset, $name($null), ())?;
                Ok(())
            }
        }
    };
    (
        #[type_id = $type_id:expr]
        $(#[width = $width:literal])?
        $(#[$m:meta])*
        $v:vis struct $name:ident($vt:vis $ty:ty);
    ) => {
        ::zigbee_macros::impl_byte! {
            $(#[width = $width])?
            $(#[$m])*
            $v struct $name($vt $ty);
        }

        impl $crate::types::codec::ZclKind for $name {
            type Value<'a> = Self;
            const TYPE_ID: $crate::types::ids::TypeId = $type_id;
            const ENCODED_SIZE: Option<usize> = Some($crate::types::codec::zcl_type!(
                @size $ty $(, $width)?
            ));
            fn read(bytes: &[u8], offset: &mut usize) -> ::byte::Result<Self> {
                ::byte::BytesExt::read_with(bytes, offset, ())
            }

            fn write(value: Self, bytes: &mut [u8], offset: &mut usize) -> ::byte::Result<()> {
                ::byte::BytesExt::write_with(bytes, offset, value, ())?;
                Ok(())
            }
        }
    };
    (@size $ty:ty) => { ::core::mem::size_of::<$ty>() };
    (@size $ty:ty, $width:literal) => { $width };
}

pub(crate) use zcl_type;

/// `bool` (2.6.2.2). `0xFF` is the non-value.
pub struct Bool;

impl ZclKind for Bool {
    type Value<'a> = bool;
    const TYPE_ID: TypeId = TypeId::Boolean;
    const ENCODED_SIZE: Option<usize> = Some(1);
    fn read(bytes: &[u8], offset: &mut usize) -> byte::Result<bool> {
        match bytes.read_with::<u8>(offset, ctx::LE)? {
            0x00 => Ok(false),
            0x01 => Ok(true),
            0xFF => Err(bad_input!("boolean non-value")),
            _ => Err(bad_input!("invalid boolean value")),
        }
    }

    fn write(value: bool, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
        bytes.write_with(offset, u8::from(value), ctx::LE)
    }
}

/// Reads a fixed-width little-endian integer, the shared body of the numeric
/// and identifier kinds.
pub(crate) fn read_le<const N: usize>(bytes: &[u8], offset: &mut usize) -> byte::Result<u64> {
    let src = bytes
        .get(*offset..*offset + N)
        .ok_or(byte::Error::Incomplete)?;
    let mut raw = [0u8; 8];
    raw[..N].copy_from_slice(src);
    *offset += N;
    Ok(u64::from_le_bytes(raw))
}

/// Writes a fixed-width little-endian integer.
pub(crate) fn write_le<const N: usize>(
    value: u64,
    bytes: &mut [u8],
    offset: &mut usize,
) -> byte::Result<()> {
    bytes
        .get_mut(*offset..*offset + N)
        .ok_or(byte::Error::Incomplete)?
        .copy_from_slice(&value.to_le_bytes()[..N]);
    *offset += N;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::descriptors::Attribute;
    use crate::types::descriptors::Cluster;
    use crate::types::ids::AttributeId;
    use crate::types::ids::ClusterId;
    use crate::types::strings::ShortText;

    const BASIC: Cluster = Cluster::new(ClusterId(0x0000), "Basic");

    // a descriptor naming a borrowed value type is still a const: the kind
    // carries no lifetime, the decoded value borrows the frame
    const MANUFACTURER_NAME: Attribute<ShortText> =
        BASIC.attribute(AttributeId(0x0004), "ManufacturerName");

    #[test]
    fn a_descriptor_decodes_a_value_borrowed_from_the_frame() {
        let frame = [0x09u8, b'z', b'i', b'g', b'b', b'e', b'e', b'-', b'r', b's'];
        let offset = &mut 0;

        let name = MANUFACTURER_NAME
            .decode(TypeId::CharacterString, &frame, offset)
            .expect("name decoded");

        assert_eq!(name.as_str(), "zigbee-rs");
        assert_eq!(*offset, frame.len());
        assert_eq!(MANUFACTURER_NAME.type_id(), TypeId::CharacterString);
    }

    // 2.6.2.10: length 0xFF is the character string non-value
    #[test]
    fn the_string_non_value_is_rejected() {
        let offset = &mut 0;
        assert!(
            MANUFACTURER_NAME
                .decode(TypeId::CharacterString, &[0xFFu8], offset)
                .is_err()
        );
    }

    #[test]
    fn variable_length_types_report_no_encoded_size() {
        assert_eq!(<ShortText as ZclKind>::ENCODED_SIZE, None);
    }

    // 2.6.2.2: 0xFF is the boolean non-value, and only 0x00/0x01 are valid
    #[test]
    fn boolean_rejects_the_non_value_and_invalid_patterns() {
        assert!(Bool::read(&[0x01], &mut 0).expect("true decoded"));
        assert!(Bool::read(&[0xFF], &mut 0).is_err());
        assert!(Bool::read(&[0x02], &mut 0).is_err());
    }
}
