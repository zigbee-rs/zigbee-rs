//! Enumeration data types.
//!
//! See ZCL Section 2.6.2.7.
//!
//! The all-ones pattern is the non-value; every other value is passed to the
//! device type, which decides whether it names a member.

use core::marker::PhantomData;

use super::codec::ZclKind;
use super::codec::read_le;
use super::codec::write_le;
use super::ids::TypeId;
use super::nullable::ZclHasNull;

/// Declares an enumeration kind and the trait a device type implements to be
/// carried by it.
macro_rules! zcl_enum {
    ($kind:ident, $trait:ident, $repr:ty, $width:literal, $type_id:expr, $null:expr, $doc:literal) => {
        #[doc = $doc]
        pub trait $trait: Sized + Copy {
            /// Returns `None` when `raw` names no member.
            fn from_raw(raw: $repr) -> Option<Self>;
            fn into_raw(self) -> $repr;
        }

        #[doc = $doc]
        ///
        /// Carries any type implementing the matching trait, so one Rust type
        /// can appear under more than one wire type.
        pub struct $kind<T>(PhantomData<T>);

        impl<T: $trait> ZclKind for $kind<T> {
            type Value<'a> = T;
            const TYPE_ID: TypeId = $type_id;
            const ENCODED_SIZE: Option<usize> = Some($width);
            fn read(bytes: &[u8], offset: &mut usize) -> byte::Result<T> {
                #[allow(clippy::cast_possible_truncation)]
                let raw = read_le::<$width>(bytes, offset)? as $repr;
                if raw == $null {
                    return Err(bad_input!("enumeration non-value"));
                }
                T::from_raw(raw).ok_or(bad_input!("invalid enumeration value"))
            }

            fn write(value: T, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
                let raw = value.into_raw();
                if raw == $null {
                    return Err(bad_input!("enumeration non-value"));
                }
                write_le::<$width>(u64::from(raw), bytes, offset)
            }
        }

        impl<T: $trait> ZclHasNull for $kind<T> {
            fn null_size(bytes: &[u8], offset: usize) -> Option<usize> {
                let raw = bytes.get(offset..offset + $width)?;
                raw.iter().all(|b| *b == 0xFF).then_some($width)
            }

            fn write_null(bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
                write_le::<$width>($null as u64, bytes, offset)
            }
        }
    };
}

zcl_enum!(
    Enum8,
    ZclEnum8,
    u8,
    1,
    TypeId::Enum8,
    0xFF,
    "`enum8` (2.6.2.7)."
);
zcl_enum!(
    Enum16,
    ZclEnum16,
    u16,
    2,
    TypeId::Enum16,
    0xFFFF,
    "`enum16` (2.6.2.7)."
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::nullable::Nullable;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PowerSource {
        Mains,
        Battery,
    }

    impl ZclEnum8 for PowerSource {
        fn from_raw(raw: u8) -> Option<Self> {
            match raw {
                0x01 => Some(Self::Mains),
                0x03 => Some(Self::Battery),
                _ => None,
            }
        }
        fn into_raw(self) -> u8 {
            match self {
                Self::Mains => 0x01,
                Self::Battery => 0x03,
            }
        }
    }

    #[test]
    fn round_trips_a_member() {
        let mut buf = [0u8; 4];
        let offset = &mut 0;
        Enum8::<PowerSource>::write(PowerSource::Battery, &mut buf, offset).expect("encoded");
        assert_eq!(&buf[..*offset], &[0x03]);
        assert_eq!(
            Enum8::<PowerSource>::read(&buf, &mut 0).expect("decoded"),
            PowerSource::Battery
        );
    }

    // 2.6.2.7: a value naming no member is rejected, as is the non-value
    #[test]
    fn unknown_and_non_values_are_rejected() {
        assert!(Enum8::<PowerSource>::read(&[0x02], &mut 0).is_err());
        assert!(Enum8::<PowerSource>::read(&[0xFF], &mut 0).is_err());
    }

    #[test]
    fn the_non_value_decodes_as_none_when_nullable() {
        assert_eq!(
            Nullable::<Enum8<PowerSource>>::read(&[0xFF], &mut 0).expect("decoded"),
            None
        );
    }
}
