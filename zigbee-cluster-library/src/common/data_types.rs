//! Data types
//!
//! See section 2.6.2
use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;

#[derive(Debug, PartialEq)]
pub enum ZclDataType<'a> {
    NoData,
    Data(DataN),
    Bool(bool),
    Bitmap(BitmapN),
    UnsignedInt(UnsignedN),
    SignedInt(SignedN),
    Enum(EnumN),
    Float(FloatN),
    String(ZclString<'a>),
    Array(&'a [ZclDataType<'a>]),
    Structure(&'a [ZclDataType<'a>]),
    Set(&'a [ZclDataType<'a>]),
    Bag(&'a [ZclDataType<'a>]),
    Time(TimeType),
    Identifier(IdentifierType),
    Misc(MiscType<'a>),
    Unknown,
}

impl<'a> TryRead<'a, u8> for ZclDataType<'a> {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x00 => Self::NoData,
            0x08..=0x0F => Self::Data(bytes.read_with(offset, identifier)?),
            0x10 => Self::Bool(bytes.read(offset)?),
            0x18..=0x1F => Self::Bitmap(bytes.read_with(offset, identifier)?),
            0x20..=0x27 => Self::UnsignedInt(bytes.read_with(offset, identifier)?),
            0x28..=0x2F => Self::SignedInt(bytes.read_with(offset, identifier)?),
            0x30 | 0x31 => Self::Enum(bytes.read_with(offset, identifier)?),
            0x38..=0x3A => Self::Float(bytes.read_with(offset, identifier)?),
            0x41..=0x44 => Self::String(bytes.read_with(offset, identifier)?),
            //0x48 => Self::Array(bytes.read_with(offset, identifier)?),
            //0x4C => Self::Structure(bytes.read_with(offset, identifier)?),
            //0x50 => Self::Set(bytes.read_with(offset, identifier)?),
            //0x51 => Self::Bag(bytes.read_with(offset, identifier)?),
            0xE0..=0xE2 => Self::Time(bytes.read_with(offset, identifier)?),
            0xE8..=0xEA => Self::Identifier(bytes.read_with(offset, identifier)?),
            0xF0 | 0xF1 => Self::Misc(bytes.read_with(offset, identifier)?),
            _ => Self::Unknown,
        };

        Ok((v, *offset))
    }
}

impl TryWrite<u8> for ZclDataType<'_> {
    fn try_write(self, _bytes: &mut [u8], _identifier: u8) -> Result<usize, ::byte::Error> {
        unimplemented!()
    }
}

/// 2.6.2.2 General Data
#[derive(Debug, PartialEq, Eq)]
pub enum DataN {
    Data8(u8),
    Data16(u16),
    Data24(u32),
    Data32(u32),
    Data40(u64),
    Data48(u64),
    Data56(u64),
    Data64(u64),
}

impl<'a> TryRead<'a, u8> for DataN {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x08 => Self::Data8(bytes.read(offset)?),
            0x09 => Self::Data16(bytes.read(offset)?),
            0x0A => Self::Data24(bytes.read(offset)?),
            0x0B => Self::Data32(bytes.read(offset)?),
            0x0C => Self::Data40(bytes.read(offset)?),
            0x0D => Self::Data48(bytes.read(offset)?),
            0x0E => Self::Data56(bytes.read(offset)?),
            0x0F => Self::Data64(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid DataN",
                })
            }
        };

        Ok((v, *offset))
    }
}

/// 2.6.2.4 Bitmap
#[derive(Debug, PartialEq, Eq)]
pub enum BitmapN {
    Bitmap8(u8),
    Bitmap16(u16),
    Bitmap24(u32),
    Bitmap32(u32),
    Bitmap40(u64),
    Bitmap48(u64),
    Bitmap56(u64),
    Bitmap64(u64),
}

impl<'a> TryRead<'a, u8> for BitmapN {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x08 => Self::Bitmap8(bytes.read(offset)?),
            0x09 => Self::Bitmap16(bytes.read(offset)?),
            0x0A => Self::Bitmap24(bytes.read(offset)?),
            0x0B => Self::Bitmap32(bytes.read(offset)?),
            0x0C => Self::Bitmap40(bytes.read(offset)?),
            0x0D => Self::Bitmap48(bytes.read(offset)?),
            0x0E => Self::Bitmap56(bytes.read(offset)?),
            0x0F => Self::Bitmap64(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid BitmapN",
                })
            }
        };

        Ok((v, *offset))
    }
}

/// 2.6.2.5 Unsigned Integer
#[derive(Debug, PartialEq, Eq)]
pub enum UnsignedN {
    Uint8(u8),
    Uint16(u16),
    Uint24(u32),
    Uint32(u32),
    Uint40(u64),
    Uint48(u64),
    Uint56(u64),
    Uint64(u64),
}

impl<'a> TryRead<'a, u8> for UnsignedN {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x20 => Self::Uint8(bytes.read(offset)?),
            0x21 => Self::Uint16(bytes.read(offset)?),
            0x22 => Self::Uint24(bytes.read(offset)?),
            0x23 => Self::Uint32(bytes.read(offset)?),
            0x24 => Self::Uint40(bytes.read(offset)?),
            0x25 => Self::Uint48(bytes.read(offset)?),
            0x26 => Self::Uint56(bytes.read(offset)?),
            0x27 => Self::Uint64(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid UnsignedN",
                })
            }
        };

        Ok((v, *offset))
    }
}

/// 2.6.2.6 Signed Integer
#[derive(Debug, PartialEq, Eq)]
pub enum SignedN {
    Int8(i8),
    Int16(i16),
    Int24(i32),
    Int32(i32),
    Int40(i64),
    Int48(i64),
    Int56(i64),
    Int64(i64),
}

impl<'a> TryRead<'a, u8> for SignedN {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x28 => Self::Int8(bytes.read(offset)?),
            0x29 => Self::Int16(bytes.read(offset)?),
            0x2A => Self::Int24(bytes.read(offset)?),
            0x2B => Self::Int32(bytes.read(offset)?),
            0x2C => Self::Int40(bytes.read(offset)?),
            0x2D => Self::Int48(bytes.read(offset)?),
            0x2E => Self::Int56(bytes.read(offset)?),
            0x2F => Self::Int64(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid SignedN",
                })
            }
        };

        Ok((v, *offset))
    }
}

/// 2.6.2.7 Enumeration
#[derive(Debug, PartialEq, Eq)]
pub enum EnumN {
    Enum8(u8),
    Enum16(u16),
}

impl<'a> TryRead<'a, u8> for EnumN {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x30 => Self::Enum8(bytes.read(offset)?),
            0x31 => Self::Enum16(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid EnumN",
                })
            }
        };

        Ok((v, *offset))
    }
}

#[derive(Debug, PartialEq)]
pub enum FloatN {
    /// 2.6.2.8 Semi-precision based on IEEE-754
    Semi(u16),
    /// 2.6.2.9 Single precision
    Single(f32),
    /// 2.6.2.10 Double precision
    Double(f64),
}

impl<'a> TryRead<'a, u8> for FloatN {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x38 => Self::Semi(bytes.read(offset)?),
            0x39 => Self::Single(bytes.read(offset)?),
            0x3A => Self::Double(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid FloatN",
                })
            }
        };

        Ok((v, *offset))
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq)]
pub enum ZclString<'a> {
    /// 2.6.2.12 Octet String
    OctetString(&'a [u8]),
    /// 2.6.2.13 Character String
    CharString(&'a str),
    /// 2.6.2.14 Long Octet String
    LongOctetString(&'a [u8]),
    /// 2.6.2.15 Long Character String
    LongCharString(&'a str),
}

impl<'a> TryRead<'a, u8> for ZclString<'a> {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            // 0x41 => Self::OctetString(bytes.read::<&'a [u8]>(offset)?),
            0x42 => Self::CharString(bytes.read::<&'a str>(offset)?),
            // 0x43 => Self::LongOctetString(bytes.read::<&'a [u8]>(offset)?),
            0x44 => Self::LongCharString(bytes.read::<&'a str>(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid ZclString",
                })
            }
        };

        Ok((v, *offset))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TimeType {
    /// 2.6.2.19 Time of day
    TimeOfDay(u32),
    /// 2.6.2.20 Date
    Date(u32),
    /// 2.6.2.21 UTC Time
    UTCTime(u32),
}

impl<'a> TryRead<'a, u8> for TimeType {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0xE0 => Self::TimeOfDay(bytes.read(offset)?),
            0xE1 => Self::Date(bytes.read(offset)?),
            0xE2 => Self::UTCTime(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid TimeType",
                })
            }
        };

        Ok((v, *offset))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IdentifierType {
    /// 2.6.2.22 Cluster ID
    ClusterId(u16),
    /// 2.6.2.23 Attribute ID
    AttributeId(u16),
    /// 2.6.2.24 `BACnet` OID (Object Identifier)
    BACnetOid(u32),
}

impl<'a> TryRead<'a, u8> for IdentifierType {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0xE8 => Self::ClusterId(bytes.read(offset)?),
            0xE9 => Self::AttributeId(bytes.read(offset)?),
            0xEA => Self::BACnetOid(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid IdentifierType",
                })
            }
        };

        Ok((v, *offset))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MiscType<'a> {
    /// 2.6.2.25 IEEE Address
    IeeeAddress(u64),
    /// 128-bit Security Key
    SecurityKey(&'a [u8; 16]),
}

impl<'a> TryRead<'a, u8> for MiscType<'a> {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0xF0 => MiscType::IeeeAddress(bytes.read(offset)?),
            // 0xF1 => MiscType::SecurityKey(bytes.read(offset)?),
            _ => {
                return Err(byte::Error::BadInput {
                    err: "invalid MiscType",
                })
            }
        };

        Ok((v, *offset))
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::ZclDataType;
    use crate::common::data_types::DataN;
    use crate::common::data_types::EnumN;
    use crate::common::data_types::FloatN;
    use crate::common::data_types::IdentifierType;
    use crate::common::data_types::MiscType;
    use crate::common::data_types::TimeType;
    use crate::common::data_types::ZclString;

    #[test]
    fn parse_nodata() {
        // given
        let input: &[u8] = &[0x00];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x00).unwrap();

        // then
        assert_eq!(len, 0);
        assert!(matches!(data, ZclDataType::NoData));
    }

    #[test]
    fn parse_data() {
        // given
        let input: &[u8] = &[0x01];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x08).unwrap();

        // then
        assert_eq!(len, 1);
        assert!(matches!(data, ZclDataType::Data(_)));
        if let ZclDataType::Data(value) = data {
            assert_eq!(value, DataN::Data8(1));
        } else {
            panic!("DataN::Data8 expected!");
        }
    }

    #[test]
    fn parse_bool() {
        // given
        let input: &[u8] = &[0x01];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x10).unwrap();

        // then
        assert_eq!(len, 1);
        assert!(matches!(data, ZclDataType::Bool(_)));
        if let ZclDataType::Bool(value) = data {
            assert_eq!(value, true);
        } else {
            panic!("ZclDataType::Bool expected!");
        }
    }

    #[test]
    fn parse_enum() {
        // given
        let input: &[u8] = &[0x1d, 0x10];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x31).unwrap();

        // then
        assert_eq!(len, 2);
        assert!(matches!(data, ZclDataType::Enum(_)));
        if let ZclDataType::Enum(value) = data {
            assert_eq!(value, EnumN::Enum16(4125u16));
        } else {
            panic!("ZclDataType::Enum expected!");
        }
    }

    #[test]
    fn parse_float() {
        // given
        let input: &[u8] = &[0x1d, 0x10];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x38).unwrap();

        // then
        assert_eq!(len, 2);
        assert!(matches!(data, ZclDataType::Float(_)));
        if let ZclDataType::Float(value) = data {
            assert_eq!(value, FloatN::Semi(4125u16));
        } else {
            panic!("ZclDataType::Float expected!");
        }
    }

    // #[test] // 💥 Parsing the input fails
    fn parse_string() {
        // given
        let input: &[u8] = &[0x48, 0x65, 0x6C, 0x6C, 0x6F];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x42).unwrap();

        // then
        assert_eq!(len, 2);
        assert!(matches!(data, ZclDataType::String(_)));
        if let ZclDataType::String(value) = data {
            assert_eq!(value, ZclString::CharString("Hello"));
        } else {
            panic!("ZclString::CharString expected!");
        }
    }

    #[test]
    fn parse_time() {
        // given
        let input: &[u8] = &[0x1d, 0x10, 0x4F, 0x46];

        // when
        let (data, _) = ZclDataType::try_read(input, 0xE2).unwrap();

        // then
        // assert_eq!(len, 2);
        assert!(matches!(data, ZclDataType::Time(_)));
        if let ZclDataType::Time(value) = data {
            assert_eq!(value, TimeType::UTCTime(1179586589));
        } else {
            panic!("TimeType::UTCTime expected!");
        }
    }

    #[test]
    fn parse_identifier() {
        // given
        let input: &[u8] = &[0x1d, 0x11];

        // when
        let (data, len) = ZclDataType::try_read(input, 0xE8).unwrap();

        // then
        assert_eq!(len, 2);
        assert!(matches!(data, ZclDataType::Identifier(_)));
        if let ZclDataType::Identifier(value) = data {
            assert_eq!(value, IdentifierType::ClusterId(4381u16));
        } else {
            panic!("IdentifierType::ClusterId expected!");
        }
    }

    #[test]
    fn parse_misc() {
        // given
        let input: &[u8] = &[
            0x1d, 0x10, 0x1d, 0x10, 0x1d, 0x10, 0x1d, 0x10, 0x1d, 0x10, 0x1d, 0x10,
        ];

        // when
        let (data, len) = ZclDataType::try_read(input, 0xF0).unwrap();

        // then
        // assert_eq!(len, 2);
        assert!(matches!(data, ZclDataType::Misc(_)));
        if let ZclDataType::Misc(value) = data {
            assert_eq!(value, MiscType::IeeeAddress(1_161_101_995_941_892_125u64));
        } else {
            panic!("MiscType::IeeeAddress expected!");
        }
    }
}
