//! Bitmap data types.
//!
//! See ZCL Section 2.6.2.6.
//!
//! Every bit pattern of a bitmap is valid and bitmaps have no non-value, so
//! decoding cannot fail on content — only on running out of bytes.

use core::marker::PhantomData;

use super::codec::ZclKind;
use super::codec::read_le;
use super::codec::write_le;
use super::ids::TypeId;

/// Declares a bitmap kind and the trait a device type implements to be carried
/// by it.
macro_rules! bitmap {
    ($kind:ident, $trait:ident, $repr:ty, $width:literal, $type_id:expr, $doc:literal) => {
        #[doc = $doc]
        pub trait $trait: Sized + Copy {
            fn from_bits(bits: $repr) -> Self;
            fn into_bits(self) -> $repr;
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
            // every pattern is a valid bitmap, and there is no non-value
            const ALL_PATTERNS_VALID: bool = true;
            fn read(bytes: &[u8], offset: &mut usize) -> byte::Result<T> {
                #[allow(clippy::cast_possible_truncation)]
                Ok(T::from_bits(read_le::<$width>(bytes, offset)? as $repr))
            }

            fn write(value: T, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
                write_le::<$width>(u64::from(value.into_bits()), bytes, offset)
            }
        }
    };
}

bitmap!(
    Bitmap8,
    ZclBitmap8,
    u8,
    1,
    TypeId::Bitmap8,
    "`bitmap8` (2.6.2.6)."
);
bitmap!(
    Bitmap16,
    ZclBitmap16,
    u16,
    2,
    TypeId::Bitmap16,
    "`bitmap16` (2.6.2.6)."
);
bitmap!(
    Bitmap24,
    ZclBitmap24,
    u32,
    3,
    TypeId::Bitmap24,
    "`bitmap24` (2.6.2.6)."
);
bitmap!(
    Bitmap32,
    ZclBitmap32,
    u32,
    4,
    TypeId::Bitmap32,
    "`bitmap32` (2.6.2.6)."
);
bitmap!(
    Bitmap40,
    ZclBitmap40,
    u64,
    5,
    TypeId::Bitmap40,
    "`bitmap40` (2.6.2.6)."
);
bitmap!(
    Bitmap48,
    ZclBitmap48,
    u64,
    6,
    TypeId::Bitmap48,
    "`bitmap48` (2.6.2.6)."
);
bitmap!(
    Bitmap56,
    ZclBitmap56,
    u64,
    7,
    TypeId::Bitmap56,
    "`bitmap56` (2.6.2.6)."
);
bitmap!(
    Bitmap64,
    ZclBitmap64,
    u64,
    8,
    TypeId::Bitmap64,
    "`bitmap64` (2.6.2.6)."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Flags(u8);

    impl ZclBitmap8 for Flags {
        fn from_bits(bits: u8) -> Self {
            Self(bits)
        }
        fn into_bits(self) -> u8 {
            self.0
        }
    }

    // the same Rust type can also be carried as a wider bitmap, which is why
    // the codec lives on the kind and not on the value
    impl ZclBitmap16 for Flags {
        fn from_bits(bits: u16) -> Self {
            #[allow(clippy::cast_possible_truncation)]
            Self(bits as u8)
        }
        fn into_bits(self) -> u16 {
            u16::from(self.0)
        }
    }

    #[test]
    fn one_type_can_be_carried_under_two_wire_types() {
        let mut buf = [0u8; 8];

        let offset = &mut 0;
        Bitmap8::<Flags>::write(Flags(0b1010), &mut buf, offset).expect("bitmap8 encoded");
        assert_eq!(&buf[..*offset], &[0b1010]);

        let offset = &mut 0;
        Bitmap16::<Flags>::write(Flags(0b1010), &mut buf, offset).expect("bitmap16 encoded");
        assert_eq!(&buf[..*offset], &[0b1010, 0x00]);

        assert_eq!(<Bitmap8<Flags> as ZclKind>::TYPE_ID, TypeId::Bitmap8);
        assert_eq!(<Bitmap16<Flags> as ZclKind>::TYPE_ID, TypeId::Bitmap16);
    }

    // 2.6.2.6: every bit pattern is valid, so only a short buffer fails
    #[test]
    fn every_pattern_decodes() {
        assert_eq!(
            Bitmap8::<Flags>::read(&[0xFF], &mut 0).expect("decoded"),
            Flags(0xFF)
        );
        assert!(Bitmap8::<Flags>::read(&[], &mut 0).is_err());
        assert!(<Bitmap8<Flags> as ZclKind>::ALL_PATTERNS_VALID);
    }

    #[test]
    fn odd_width_bitmaps_use_their_nominal_width() {
        assert_eq!(<Bitmap24<Flags2> as ZclKind>::ENCODED_SIZE, Some(3));
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Flags2(u32);

    impl ZclBitmap24 for Flags2 {
        fn from_bits(bits: u32) -> Self {
            Self(bits)
        }
        fn into_bits(self) -> u32 {
            self.0
        }
    }
}
