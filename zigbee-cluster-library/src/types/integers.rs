//! Integer data types.
//!
//! See ZCL Section 2.6.2.
//!
//! One newtype per wire type rather than a bare Rust integer: the
//! specification gives `uint16`, `bitmap16` and `enum16` the same width but
//! different type identifiers, so the identifier has to live on the type.

use super::codec::zcl_type;
use super::ids::TypeId;

zcl_type! {
    #[type_id = TypeId::Uint8]
    #[null = 0xff]
    /// `uint8` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint8(pub u8);
}

zcl_type! {
    #[type_id = TypeId::Uint16]
    #[null = 0xffff]
    /// `uint16` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint16(pub u16);
}

zcl_type! {
    #[type_id = TypeId::Uint24]
    #[width = 3]
    #[null = 0x00ff_ffff]
    /// `uint24` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint24(pub u32);
}

zcl_type! {
    #[type_id = TypeId::Uint32]
    #[null = 0xffff_ffff]
    /// `uint32` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint32(pub u32);
}

zcl_type! {
    #[type_id = TypeId::Uint40]
    #[width = 5]
    #[null = 0x00ff_ffff_ffff]
    /// `uint40` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint40(pub u64);
}

zcl_type! {
    #[type_id = TypeId::Uint48]
    #[width = 6]
    #[null = 0x0000_ffff_ffff_ffff]
    /// `uint48` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint48(pub u64);
}

zcl_type! {
    #[type_id = TypeId::Uint56]
    #[width = 7]
    #[null = 0x00ff_ffff_ffff_ffff]
    /// `uint56` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint56(pub u64);
}

zcl_type! {
    #[type_id = TypeId::Uint64]
    #[null = 0xffff_ffff_ffff_ffff]
    /// `uint64` (2.6.2.4).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Uint64(pub u64);
}

zcl_type! {
    #[type_id = TypeId::Int8]
    #[null = i8::MIN]
    /// `int8` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int8(pub i8);
}

zcl_type! {
    #[type_id = TypeId::Int16]
    #[null = i16::MIN]
    /// `int16` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int16(pub i16);
}

zcl_type! {
    #[type_id = TypeId::Int24]
    #[width = 3]
    #[null = -0x0080_0000]
    /// `int24` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int24(pub i32);
}

zcl_type! {
    #[type_id = TypeId::Int32]
    #[null = i32::MIN]
    /// `int32` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int32(pub i32);
}

zcl_type! {
    #[type_id = TypeId::Int40]
    #[width = 5]
    #[null = -0x0080_0000_0000]
    /// `int40` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int40(pub i64);
}

zcl_type! {
    #[type_id = TypeId::Int48]
    #[width = 6]
    #[null = -0x0000_8000_0000_0000]
    /// `int48` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int48(pub i64);
}

zcl_type! {
    #[type_id = TypeId::Int56]
    #[width = 7]
    #[null = -0x0080_0000_0000_0000]
    /// `int56` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int56(pub i64);
}

zcl_type! {
    #[type_id = TypeId::Int64]
    #[null = i64::MIN]
    /// `int64` (2.6.2.5).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
    pub struct Int64(pub i64);
}

#[cfg(test)]
mod tests {
    use byte::BytesExt;

    use super::*;
    use crate::types::codec::ZclKind;
    use crate::types::nullable::Nullable;

    // 2.6.2.5: the odd widths occupy exactly their nominal number of bytes and
    // are two's complement, so the sign has to survive the narrow encoding
    #[test]
    fn odd_width_integers_round_trip_signed() {
        for value in [0i32, 1, -1, 8_388_607, -8_388_607, -12_345] {
            let mut buf = [0u8; 8];
            let offset = &mut 0;
            buf.write_with(offset, Int24(value), ())
                .expect("int24 encoded");
            assert_eq!(*offset, 3);

            let read = &mut 0;
            let decoded: Int24 = buf.read_with(read, ()).expect("int24 decoded");
            assert_eq!(decoded, Int24(value));
            assert_eq!(*read, 3);
        }
    }

    #[test]
    fn odd_width_integers_round_trip_unsigned() {
        for value in [0u64, 1, 0x00ff_ffff_fffe, u64::from(u32::MAX)] {
            let mut buf = [0u8; 8];
            let offset = &mut 0;
            buf.write_with(offset, Uint40(value), ())
                .expect("uint40 encoded");
            assert_eq!(*offset, 5);

            let read = &mut 0;
            let decoded: Uint40 = buf.read_with(read, ()).expect("uint40 decoded");
            assert_eq!(decoded, Uint40(value));
        }
    }

    // 2.6.2.4 / 2.6.2.5: the all-ones (unsigned) and most-negative (signed)
    // patterns mean "unknown", so a plain attribute must not carry them
    #[test]
    fn the_non_value_is_rejected_by_the_zcl_layer() {
        assert!(Uint8::read(&[0xff], &mut 0).is_err());
        assert!(Int16::read(&[0x00, 0x80], &mut 0).is_err());
        assert!(Int24::read(&[0x00, 0x00, 0x80], &mut 0).is_err());
        assert!(Uint8::write(Uint8(0xff), &mut [0u8; 4], &mut 0).is_err());
    }

    // the `byte` codec is the raw wire layer and can still represent the
    // non-value, which is what lets `Nullable` write it
    #[test]
    fn the_raw_codec_still_represents_the_non_value() {
        let mut buf = [0u8; 4];
        let offset = &mut 0;
        buf.write_with(offset, Uint8(0xff), ())
            .expect("raw encoded");
        assert_eq!(&buf[..*offset], &[0xff]);

        assert_eq!(
            Nullable::<Uint8>::read(&buf, &mut 0).expect("decoded as null"),
            None
        );
    }

    #[test]
    fn encoded_size_matches_the_wire_width() {
        assert_eq!(<Uint8 as ZclKind>::ENCODED_SIZE, Some(1));
        assert_eq!(<Int16 as ZclKind>::ENCODED_SIZE, Some(2));
        assert_eq!(<Int24 as ZclKind>::ENCODED_SIZE, Some(3));
        assert_eq!(<Uint48 as ZclKind>::ENCODED_SIZE, Some(6));
        assert_eq!(<Int64 as ZclKind>::ENCODED_SIZE, Some(8));
    }
}
