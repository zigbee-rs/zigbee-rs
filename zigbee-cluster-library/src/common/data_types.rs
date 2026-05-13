//! Data types
//!
//! See section 2.6.2
use core::mem::size_of;

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;

fn read_uint(bytes: &[u8], len: usize) -> Result<u64, byte::Error> {
    if len == 0 || len > size_of::<u64>() {
        return Err(bad_input!("invalid integer width"));
    }

    let src = bytes.get(..len).ok_or(byte::Error::Incomplete)?;
    let mut raw = [0u8; 8];
    raw[..len].copy_from_slice(src);
    Ok(u64::from_le_bytes(raw))
}

fn read_int(bytes: &[u8], len: usize) -> Result<i64, byte::Error> {
    if len == 0 || len > size_of::<i64>() {
        return Err(bad_input!("invalid integer width"));
    }

    let value = read_uint(bytes, len)?;
    let shift = 64 - (len * 8);
    Ok((value << shift) as i64 >> shift)
}

fn write_uint(bytes: &mut [u8], value: u64, len: usize) -> Result<usize, byte::Error> {
    if len == 0 || len > size_of::<u64>() {
        return Err(bad_input!("invalid integer width"));
    }

    let max = if len == size_of::<u64>() {
        u64::MAX
    } else {
        (1u64 << (len * 8)) - 1
    };
    if value > max {
        return Err(bad_input!("integer value exceeds encoded width"));
    }

    bytes
        .get_mut(..len)
        .ok_or(byte::Error::Incomplete)?
        .copy_from_slice(&value.to_le_bytes()[..len]);
    Ok(len)
}

fn write_int(bytes: &mut [u8], value: i64, len: usize) -> Result<usize, byte::Error> {
    if len == 0 || len > size_of::<i64>() {
        return Err(bad_input!("invalid integer width"));
    }

    let bits = len * 8;
    let min = -(1i128 << (bits - 1));
    let max = (1i128 << (bits - 1)) - 1;
    let value_i128 = i128::from(value);
    if value_i128 < min || value_i128 > max {
        return Err(bad_input!("integer value exceeds encoded width"));
    }

    bytes
        .get_mut(..len)
        .ok_or(byte::Error::Incomplete)?
        .copy_from_slice(&value.to_le_bytes()[..len]);
    Ok(len)
}

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
    /// Not implemented
    Array(&'a [Self]),
    /// Not implemented
    Structure(&'a [Self]),
    /// Not implemented
    Set(&'a [Self]),
    /// Not implemented
    Bag(&'a [Self]),
    Time(TimeType),
    Identifier(IdentifierType),
    Misc(MiscType<'a>),
    Unknown,
}

impl ZclDataType<'_> {
    pub fn type_id(&self) -> Result<u8, byte::Error> {
        match self {
            Self::NoData => Ok(0x00),
            Self::Data(value) => Ok(value.type_id()),
            Self::Bool(_) => Ok(0x10),
            Self::Bitmap(value) => Ok(value.type_id()),
            Self::UnsignedInt(value) => Ok(value.type_id()),
            Self::SignedInt(value) => Ok(value.type_id()),
            Self::Enum(value) => Ok(value.type_id()),
            Self::Float(value) => Ok(value.type_id()),
            Self::String(value) => Ok(value.type_id()),
            Self::Time(value) => Ok(value.type_id()),
            Self::Identifier(value) => Ok(value.type_id()),
            Self::Misc(value) => Ok(value.type_id()),
            Self::Array(_) | Self::Structure(_) | Self::Set(_) | Self::Bag(_) => {
                Err(bad_input!("unimplemented ZCL data type"))
            }
            Self::Unknown => Err(bad_input!("unsupported ZCL data type")),
        }
    }
}

impl<'a> TryRead<'a, u8> for ZclDataType<'a> {
    fn try_read(bytes: &'a [u8], identifier: u8) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let v = match identifier {
            0x00 => Self::NoData,
            0x08..=0x0F => Self::Data(bytes.read_with(offset, identifier)?),
            0x10 => {
                let raw: u8 = bytes.read(offset)?;
                match raw {
                    0x00 => Self::Bool(false),
                    0x01 => Self::Bool(true),
                    _ => return Err(bad_input!("invalid ZCL boolean value")),
                }
            }
            0x18..=0x1F => Self::Bitmap(bytes.read_with(offset, identifier)?),
            0x20..=0x27 => Self::UnsignedInt(bytes.read_with(offset, identifier)?),
            0x28..=0x2F => Self::SignedInt(bytes.read_with(offset, identifier)?),
            0x30 | 0x31 => Self::Enum(bytes.read_with(offset, identifier)?),
            0x38..=0x3A => Self::Float(bytes.read_with(offset, identifier)?),
            0x41..=0x44 => Self::String(bytes.read_with(offset, identifier)?),
            // still need to handle these variants
            // could use an iterator/lazy parser or over-engineer a visitor pattern maybe?
            //0x48 => Self::Array(bytes.read_with(offset, identifier)?),
            //0x4C => Self::Structure(bytes.read_with(offset, identifier)?),
            //0x50 => Self::Set(bytes.read_with(offset, identifier)?),
            //0x51 => Self::Bag(bytes.read_with(offset, identifier)?),
            0xE0..=0xE2 => Self::Time(bytes.read_with(offset, identifier)?),
            0xE8..=0xEA => Self::Identifier(bytes.read_with(offset, identifier)?),
            0xF0 | 0xF1 => Self::Misc(bytes.read_with(offset, identifier)?),
            _ => return Err(bad_input!("unsupported ZCL data type")),
        };

        Ok((v, *offset))
    }
}

impl TryWrite<u8> for ZclDataType<'_> {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match self {
            Self::NoData if identifier == 0x00 => Ok(0),
            Self::Data(value) => value.try_write(bytes, identifier),
            Self::Bool(value) if identifier == 0x10 => {
                *bytes.first_mut().ok_or(byte::Error::Incomplete)? = u8::from(value);
                Ok(1)
            }
            Self::Bitmap(value) => value.try_write(bytes, identifier),
            Self::UnsignedInt(value) => value.try_write(bytes, identifier),
            Self::SignedInt(value) => value.try_write(bytes, identifier),
            Self::Enum(value) => value.try_write(bytes, identifier),
            Self::Float(value) => value.try_write(bytes, identifier),
            Self::String(value) => value.try_write(bytes, identifier),
            Self::Time(value) => value.try_write(bytes, identifier),
            Self::Identifier(value) => value.try_write(bytes, identifier),
            Self::Misc(value) => value.try_write(bytes, identifier),
            _ => Err(bad_input!("ZCL data type does not match identifier")),
        }
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
            0x0A => {
                *offset = 3;
                Self::Data24(
                    u32::try_from(read_uint(bytes, 3)?)
                        .map_err(|_| bad_input!("invalid Data24"))?,
                )
            }
            0x0B => Self::Data32(bytes.read(offset)?),
            0x0C => {
                *offset = 5;
                Self::Data40(read_uint(bytes, 5)?)
            }
            0x0D => {
                *offset = 6;
                Self::Data48(read_uint(bytes, 6)?)
            }
            0x0E => {
                *offset = 7;
                Self::Data56(read_uint(bytes, 7)?)
            }
            0x0F => Self::Data64(bytes.read(offset)?),
            _ => return Err(bad_input!("invalid DataN")),
        };

        Ok((v, *offset))
    }
}

impl DataN {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::Data8(_) => 0x08,
            Self::Data16(_) => 0x09,
            Self::Data24(_) => 0x0A,
            Self::Data32(_) => 0x0B,
            Self::Data40(_) => 0x0C,
            Self::Data48(_) => 0x0D,
            Self::Data56(_) => 0x0E,
            Self::Data64(_) => 0x0F,
        }
    }
}

impl TryWrite<u8> for DataN {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x08, Self::Data8(value)) => value.try_write(bytes, byte::LE),
            (0x09, Self::Data16(value)) => value.try_write(bytes, byte::LE),
            (0x0A, Self::Data24(value)) => write_uint(bytes, u64::from(value), 3),
            (0x0B, Self::Data32(value)) => value.try_write(bytes, byte::LE),
            (0x0C, Self::Data40(value)) => write_uint(bytes, value, 5),
            (0x0D, Self::Data48(value)) => write_uint(bytes, value, 6),
            (0x0E, Self::Data56(value)) => write_uint(bytes, value, 7),
            (0x0F, Self::Data64(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("DataN does not match identifier")),
        }
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
            0x18 => Self::Bitmap8(bytes.read(offset)?),
            0x19 => Self::Bitmap16(bytes.read(offset)?),
            0x1A => {
                *offset = 3;
                Self::Bitmap24(
                    u32::try_from(read_uint(bytes, 3)?)
                        .map_err(|_| bad_input!("invalid Bitmap24"))?,
                )
            }
            0x1B => Self::Bitmap32(bytes.read(offset)?),
            0x1C => {
                *offset = 5;
                Self::Bitmap40(read_uint(bytes, 5)?)
            }
            0x1D => {
                *offset = 6;
                Self::Bitmap48(read_uint(bytes, 6)?)
            }
            0x1E => {
                *offset = 7;
                Self::Bitmap56(read_uint(bytes, 7)?)
            }
            0x1F => Self::Bitmap64(bytes.read(offset)?),
            _ => return Err(bad_input!("invalid BitmapN")),
        };

        Ok((v, *offset))
    }
}

impl BitmapN {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::Bitmap8(_) => 0x18,
            Self::Bitmap16(_) => 0x19,
            Self::Bitmap24(_) => 0x1A,
            Self::Bitmap32(_) => 0x1B,
            Self::Bitmap40(_) => 0x1C,
            Self::Bitmap48(_) => 0x1D,
            Self::Bitmap56(_) => 0x1E,
            Self::Bitmap64(_) => 0x1F,
        }
    }
}

impl TryWrite<u8> for BitmapN {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x18, Self::Bitmap8(value)) => value.try_write(bytes, byte::LE),
            (0x19, Self::Bitmap16(value)) => value.try_write(bytes, byte::LE),
            (0x1A, Self::Bitmap24(value)) => write_uint(bytes, u64::from(value), 3),
            (0x1B, Self::Bitmap32(value)) => value.try_write(bytes, byte::LE),
            (0x1C, Self::Bitmap40(value)) => write_uint(bytes, value, 5),
            (0x1D, Self::Bitmap48(value)) => write_uint(bytes, value, 6),
            (0x1E, Self::Bitmap56(value)) => write_uint(bytes, value, 7),
            (0x1F, Self::Bitmap64(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("BitmapN does not match identifier")),
        }
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
            0x22 => {
                *offset = 3;
                Self::Uint24(
                    u32::try_from(read_uint(bytes, 3)?)
                        .map_err(|_| bad_input!("invalid Uint24"))?,
                )
            }
            0x23 => Self::Uint32(bytes.read(offset)?),
            0x24 => {
                *offset = 5;
                Self::Uint40(read_uint(bytes, 5)?)
            }
            0x25 => {
                *offset = 6;
                Self::Uint48(read_uint(bytes, 6)?)
            }
            0x26 => {
                *offset = 7;
                Self::Uint56(read_uint(bytes, 7)?)
            }
            0x27 => Self::Uint64(bytes.read(offset)?),
            _ => return Err(bad_input!("invalid UnsignedN")),
        };

        Ok((v, *offset))
    }
}

impl UnsignedN {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::Uint8(_) => 0x20,
            Self::Uint16(_) => 0x21,
            Self::Uint24(_) => 0x22,
            Self::Uint32(_) => 0x23,
            Self::Uint40(_) => 0x24,
            Self::Uint48(_) => 0x25,
            Self::Uint56(_) => 0x26,
            Self::Uint64(_) => 0x27,
        }
    }
}

impl TryWrite<u8> for UnsignedN {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x20, Self::Uint8(value)) => value.try_write(bytes, byte::LE),
            (0x21, Self::Uint16(value)) => value.try_write(bytes, byte::LE),
            (0x22, Self::Uint24(value)) => write_uint(bytes, u64::from(value), 3),
            (0x23, Self::Uint32(value)) => value.try_write(bytes, byte::LE),
            (0x24, Self::Uint40(value)) => write_uint(bytes, value, 5),
            (0x25, Self::Uint48(value)) => write_uint(bytes, value, 6),
            (0x26, Self::Uint56(value)) => write_uint(bytes, value, 7),
            (0x27, Self::Uint64(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("UnsignedN does not match identifier")),
        }
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
            0x2A => {
                *offset = 3;
                Self::Int24(
                    i32::try_from(read_int(bytes, 3)?).map_err(|_| bad_input!("invalid Int24"))?,
                )
            }
            0x2B => Self::Int32(bytes.read(offset)?),
            0x2C => {
                *offset = 5;
                Self::Int40(read_int(bytes, 5)?)
            }
            0x2D => {
                *offset = 6;
                Self::Int48(read_int(bytes, 6)?)
            }
            0x2E => {
                *offset = 7;
                Self::Int56(read_int(bytes, 7)?)
            }
            0x2F => Self::Int64(bytes.read(offset)?),
            _ => return Err(bad_input!("invalid SignedN")),
        };

        Ok((v, *offset))
    }
}

impl SignedN {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::Int8(_) => 0x28,
            Self::Int16(_) => 0x29,
            Self::Int24(_) => 0x2A,
            Self::Int32(_) => 0x2B,
            Self::Int40(_) => 0x2C,
            Self::Int48(_) => 0x2D,
            Self::Int56(_) => 0x2E,
            Self::Int64(_) => 0x2F,
        }
    }
}

impl TryWrite<u8> for SignedN {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x28, Self::Int8(value)) => value.try_write(bytes, byte::LE),
            (0x29, Self::Int16(value)) => value.try_write(bytes, byte::LE),
            (0x2A, Self::Int24(value)) => write_int(bytes, i64::from(value), 3),
            (0x2B, Self::Int32(value)) => value.try_write(bytes, byte::LE),
            (0x2C, Self::Int40(value)) => write_int(bytes, value, 5),
            (0x2D, Self::Int48(value)) => write_int(bytes, value, 6),
            (0x2E, Self::Int56(value)) => write_int(bytes, value, 7),
            (0x2F, Self::Int64(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("SignedN does not match identifier")),
        }
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
                return Err(bad_input!("invalid EnumN"));
            }
        };

        Ok((v, *offset))
    }
}

impl EnumN {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::Enum8(_) => 0x30,
            Self::Enum16(_) => 0x31,
        }
    }
}

impl TryWrite<u8> for EnumN {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x30, Self::Enum8(value)) => value.try_write(bytes, byte::LE),
            (0x31, Self::Enum16(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("EnumN does not match identifier")),
        }
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
                return Err(bad_input!("invalid FloatN"));
            }
        };

        Ok((v, *offset))
    }
}

impl FloatN {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::Semi(_) => 0x38,
            Self::Single(_) => 0x39,
            Self::Double(_) => 0x3A,
        }
    }
}

impl TryWrite<u8> for FloatN {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x38, Self::Semi(value)) => value.try_write(bytes, byte::LE),
            (0x39, Self::Single(value)) => value.try_write(bytes, byte::LE),
            (0x3A, Self::Double(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("FloatN does not match identifier")),
        }
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
        match identifier {
            0x41 => {
                let len = usize::from(*bytes.first().ok_or(byte::Error::Incomplete)?);
                if len == 0xff {
                    return Err(bad_input!("invalid octet string length"));
                }
                let end = 1 + len;
                let value = bytes.get(1..end).ok_or(byte::Error::Incomplete)?;
                Ok((Self::OctetString(value), end))
            }
            0x42 => {
                let len = usize::from(*bytes.first().ok_or(byte::Error::Incomplete)?);
                if len == 0xff {
                    return Err(bad_input!("invalid character string length"));
                }
                let end = 1 + len;
                let raw = bytes.get(1..end).ok_or(byte::Error::Incomplete)?;
                let value = core::str::from_utf8(raw)
                    .map_err(|_| bad_input!("invalid character string"))?;
                Ok((Self::CharString(value), end))
            }
            0x43 => {
                if bytes.len() < 2 {
                    return Err(byte::Error::Incomplete);
                }
                let len = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
                if len == 0xffff {
                    return Err(bad_input!("invalid long octet string length"));
                }
                let end = 2 + len;
                let value = bytes.get(2..end).ok_or(byte::Error::Incomplete)?;
                Ok((Self::LongOctetString(value), end))
            }
            0x44 => {
                if bytes.len() < 2 {
                    return Err(byte::Error::Incomplete);
                }
                let len = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
                if len == 0xffff {
                    return Err(bad_input!("invalid long character string length"));
                }
                let end = 2 + len;
                let raw = bytes.get(2..end).ok_or(byte::Error::Incomplete)?;
                let value = core::str::from_utf8(raw)
                    .map_err(|_| bad_input!("invalid long character string"))?;
                Ok((Self::LongCharString(value), end))
            }
            _ => Err(bad_input!("invalid ZclString")),
        }
    }
}

impl ZclString<'_> {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::OctetString(_) => 0x41,
            Self::CharString(_) => 0x42,
            Self::LongOctetString(_) => 0x43,
            Self::LongCharString(_) => 0x44,
        }
    }
}

impl TryWrite<u8> for ZclString<'_> {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0x41, Self::OctetString(value)) => {
                let len =
                    u8::try_from(value.len()).map_err(|_| bad_input!("octet string too long"))?;
                if len == 0xff {
                    return Err(bad_input!("octet string length 0xFF is reserved as null"));
                }
                *bytes.first_mut().ok_or(byte::Error::Incomplete)? = len;
                let end = 1 + usize::from(len);
                bytes
                    .get_mut(1..end)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(value);
                Ok(end)
            }
            (0x42, Self::CharString(value)) => {
                let raw = value.as_bytes();
                let len =
                    u8::try_from(raw.len()).map_err(|_| bad_input!("character string too long"))?;
                if len == 0xff {
                    return Err(bad_input!(
                        "character string length 0xFF is reserved as null"
                    ));
                }
                *bytes.first_mut().ok_or(byte::Error::Incomplete)? = len;
                let end = 1 + usize::from(len);
                bytes
                    .get_mut(1..end)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(raw);
                Ok(end)
            }
            (0x43, Self::LongOctetString(value)) => {
                let len = u16::try_from(value.len())
                    .map_err(|_| bad_input!("long octet string too long"))?;
                if len == 0xffff {
                    return Err(bad_input!(
                        "long octet string length 0xFFFF is reserved as null"
                    ));
                }
                bytes
                    .get_mut(..2)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(&len.to_le_bytes());
                let end = 2 + usize::from(len);
                bytes
                    .get_mut(2..end)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(value);
                Ok(end)
            }
            (0x44, Self::LongCharString(value)) => {
                let raw = value.as_bytes();
                let len = u16::try_from(raw.len())
                    .map_err(|_| bad_input!("long character string too long"))?;
                if len == 0xffff {
                    return Err(bad_input!(
                        "long character string length 0xFFFF is reserved as null"
                    ));
                }
                bytes
                    .get_mut(..2)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(&len.to_le_bytes());
                let end = 2 + usize::from(len);
                bytes
                    .get_mut(2..end)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(raw);
                Ok(end)
            }
            _ => Err(bad_input!("ZclString does not match identifier")),
        }
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
                return Err(bad_input!("invalid TimeType"));
            }
        };

        Ok((v, *offset))
    }
}

impl TimeType {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::TimeOfDay(_) => 0xE0,
            Self::Date(_) => 0xE1,
            Self::UTCTime(_) => 0xE2,
        }
    }
}

impl TryWrite<u8> for TimeType {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0xE0, Self::TimeOfDay(value))
            | (0xE1, Self::Date(value))
            | (0xE2, Self::UTCTime(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("TimeType does not match identifier")),
        }
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
                return Err(bad_input!("invalid IdentifierType"));
            }
        };

        Ok((v, *offset))
    }
}

impl IdentifierType {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::ClusterId(_) => 0xE8,
            Self::AttributeId(_) => 0xE9,
            Self::BACnetOid(_) => 0xEA,
        }
    }
}

impl TryWrite<u8> for IdentifierType {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0xE8, Self::ClusterId(value)) | (0xE9, Self::AttributeId(value)) => {
                value.try_write(bytes, byte::LE)
            }
            (0xEA, Self::BACnetOid(value)) => value.try_write(bytes, byte::LE),
            _ => Err(bad_input!("IdentifierType does not match identifier")),
        }
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
            0xF1 => {
                let raw: &'a [u8; 16] = bytes
                    .get(..16)
                    .ok_or(byte::Error::Incomplete)?
                    .try_into()
                    .map_err(|_| bad_input!("invalid security key length"))?;
                *offset = 16;
                MiscType::SecurityKey(raw)
            }
            _ => {
                return Err(bad_input!("invalid MiscType"));
            }
        };

        Ok((v, *offset))
    }
}

impl MiscType<'_> {
    pub fn type_id(&self) -> u8 {
        match self {
            Self::IeeeAddress(_) => 0xF0,
            Self::SecurityKey(_) => 0xF1,
        }
    }
}

impl TryWrite<u8> for MiscType<'_> {
    fn try_write(self, bytes: &mut [u8], identifier: u8) -> Result<usize, ::byte::Error> {
        match (identifier, self) {
            (0xF0, Self::IeeeAddress(value)) => value.try_write(bytes, byte::LE),
            (0xF1, Self::SecurityKey(value)) => {
                bytes
                    .get_mut(..16)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(value);
                Ok(16)
            }
            _ => Err(bad_input!("MiscType does not match identifier")),
        }
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use super::ZclDataType;
    use crate::common::data_types::DataN;
    use crate::common::data_types::EnumN;
    use crate::common::data_types::FloatN;
    use crate::common::data_types::IdentifierType;
    use crate::common::data_types::MiscType;
    use crate::common::data_types::SignedN;
    use crate::common::data_types::TimeType;
    use crate::common::data_types::UnsignedN;
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
            assert!(value);
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

    #[test]
    fn parse_string() {
        // given
        let input: &[u8] = &[0x05, b'H', b'e', b'l', b'l', b'o'];

        // when
        let (data, len) = ZclDataType::try_read(input, 0x42).unwrap();

        // then
        assert_eq!(len, input.len());
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
            assert_eq!(value, TimeType::UTCTime(1_179_586_589));
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
        assert_eq!(len, 8);
        assert!(matches!(data, ZclDataType::Misc(_)));
        if let ZclDataType::Misc(value) = data {
            assert_eq!(value, MiscType::IeeeAddress(1_161_101_995_941_892_125u64));
        } else {
            panic!("MiscType::IeeeAddress expected!");
        }
    }

    #[test]
    fn octet_string_roundtrips() {
        let mut buf = [0u8; 8];
        let payload: &[u8] = &[0xDE, 0xAD, 0xBE];

        let write_len = ZclDataType::String(ZclString::OctetString(payload))
            .try_write(&mut buf, 0x41)
            .unwrap();
        assert_eq!(write_len, 4);
        assert_eq!(&buf[..write_len], &[0x03, 0xDE, 0xAD, 0xBE]);

        let (value, read_len) = ZclDataType::try_read(&buf[..write_len], 0x41).unwrap();
        assert_eq!(read_len, 4);
        assert_eq!(value, ZclDataType::String(ZclString::OctetString(payload)));
    }

    #[test]
    fn long_octet_string_roundtrips() {
        let mut buf = [0u8; 8];
        let payload: &[u8] = &[0xCA, 0xFE];

        let write_len = ZclDataType::String(ZclString::LongOctetString(payload))
            .try_write(&mut buf, 0x43)
            .unwrap();
        assert_eq!(write_len, 4);
        assert_eq!(&buf[..write_len], &[0x02, 0x00, 0xCA, 0xFE]);

        let (value, read_len) = ZclDataType::try_read(&buf[..write_len], 0x43).unwrap();
        assert_eq!(read_len, 4);
        assert_eq!(
            value,
            ZclDataType::String(ZclString::LongOctetString(payload))
        );
    }

    #[test]
    fn long_char_string_roundtrips() {
        let mut buf = [0u8; 16];

        let write_len = ZclDataType::String(ZclString::LongCharString("Hello"))
            .try_write(&mut buf, 0x44)
            .unwrap();
        assert_eq!(write_len, 7);
        assert_eq!(
            &buf[..write_len],
            &[0x05, 0x00, b'H', b'e', b'l', b'l', b'o']
        );

        let (value, read_len) = ZclDataType::try_read(&buf[..write_len], 0x44).unwrap();
        assert_eq!(read_len, 7);
        assert_eq!(
            value,
            ZclDataType::String(ZclString::LongCharString("Hello"))
        );
    }

    #[test]
    fn char_string_null_indicator_rejected_on_write() {
        let raw = [b'a'; 255];
        let s = core::str::from_utf8(&raw).unwrap();
        let mut buf = [0u8; 260];
        assert!(
            ZclDataType::String(ZclString::CharString(s))
                .try_write(&mut buf, 0x42)
                .is_err()
        );
    }

    #[test]
    fn octet_string_null_indicator_rejected_on_write() {
        let payload = [0u8; 255];
        let mut buf = [0u8; 260];
        assert!(
            ZclDataType::String(ZclString::OctetString(&payload))
                .try_write(&mut buf, 0x41)
                .is_err()
        );
    }

    #[test]
    fn writes_non_standard_width_integers_without_truncation() {
        let mut buf = [0u8; 8];

        let len = ZclDataType::UnsignedInt(UnsignedN::Uint24(0x12_34_56))
            .try_write(&mut buf, 0x22)
            .unwrap();
        assert_eq!(len, 3);
        assert_eq!(&buf[..len], &[0x56, 0x34, 0x12]);

        let (value, read) = ZclDataType::try_read(&buf[..len], 0x22).unwrap();
        assert_eq!(read, 3);
        assert_eq!(
            value,
            ZclDataType::UnsignedInt(UnsignedN::Uint24(0x12_34_56))
        );

        let len = ZclDataType::SignedInt(SignedN::Int24(-2))
            .try_write(&mut buf, 0x2A)
            .unwrap();
        assert_eq!(len, 3);
        assert_eq!(&buf[..len], &[0xFE, 0xFF, 0xFF]);

        let (value, read) = ZclDataType::try_read(&buf[..len], 0x2A).unwrap();
        assert_eq!(read, 3);
        assert_eq!(value, ZclDataType::SignedInt(SignedN::Int24(-2)));
    }

    #[test]
    fn non_standard_width_writes_reject_out_of_range_values() {
        let mut buf = [0u8; 8];

        assert!(
            ZclDataType::UnsignedInt(UnsignedN::Uint24(0x01_00_00_00))
                .try_write(&mut buf, 0x22)
                .is_err()
        );
        assert!(
            ZclDataType::SignedInt(SignedN::Int24(0x0080_0000))
                .try_write(&mut buf, 0x2A)
                .is_err()
        );
        assert!(
            ZclDataType::SignedInt(SignedN::Int24(-0x0080_0001))
                .try_write(&mut buf, 0x2A)
                .is_err()
        );
    }
}
