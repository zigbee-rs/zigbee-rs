use super::collections::CollectionKind;
use super::collections::MaybeCollectionRef;
use super::collections::MaybeStructRef;
use super::collections::ZclCollectionRef;
use super::collections::ZclStructRef;
use super::error::ZclError;
use super::ids::AttributeId;
use super::ids::ClusterId;
use super::ids::RawTypeId;
use super::ids::TypeId;
use super::schema::BacnetOid;
use super::schema::IeeeAddress;
use super::schema::SecurityKey;
use super::schema::UtcTime;
use super::schema::ZclDate;
use super::schema::ZclTimeOfDay;
use super::strings::ZclText;

/// Dynamic ZCL value decoded without compile-time schema knowledge.
/// Used for gateways, bridges, logging, and commissioning tools.
#[non_exhaustive]
#[derive(Debug, PartialEq)]
pub enum ZclValueRef<'a> {
    NoData,
    Bool(bool),
    Uint8(u8),
    Uint16(u16),
    Uint24(u32),
    Uint32(u32),
    Uint40(u64),
    Uint48(u64),
    Uint56(u64),
    Uint64(u64),
    Int8(i8),
    Int16(i16),
    Int24(i32),
    Int32(i32),
    Int40(i64),
    Int48(i64),
    Int56(i64),
    Int64(i64),
    Data8(u8),
    Data16(u16),
    Data32(u32),
    Data64(u64),
    Bitmap8(u8),
    Bitmap16(u16),
    Bitmap32(u32),
    Bitmap64(u64),
    Enum8(u8),
    Enum16(u16),
    SemiFloat(u16),
    Float(f32),
    Double(f64),
    ShortText(Option<ZclText<'a>>),
    LongText(Option<ZclText<'a>>),
    ShortOctets(Option<&'a [u8]>),
    LongOctets(Option<&'a [u8]>),
    Array(MaybeCollectionRef<'a>),
    Set(MaybeCollectionRef<'a>),
    Bag(MaybeCollectionRef<'a>),
    Structure(MaybeStructRef<'a>),
    TimeOfDay(ZclTimeOfDay),
    Date(ZclDate),
    UtcTime(UtcTime),
    ClusterId(ClusterId),
    AttributeId(AttributeId),
    BacnetOid(BacnetOid),
    IeeeAddress(IeeeAddress),
    SecurityKey(SecurityKey),
    /// Known fixed-width ZCL type that this dynamic enum does not model as a
    /// semantic Rust value. The raw bytes are preserved exactly.
    RawFixed(TypeId, &'a [u8]),
}

impl<'a> ZclValueRef<'a> {
    /// Decode a ZCL value given its wire `TypeId` and the payload bytes.
    /// Returns `(value, bytes_consumed)`.
    ///
    /// For variable-length types (strings, collections, structures), scans the
    /// payload to determine exact byte count. Unknown raw type bytes cannot be
    /// decoded through this API because `TypeId` preserves only known variants.
    pub fn decode_with_type(type_id: TypeId, bytes: &'a [u8]) -> Result<(Self, usize), ZclError> {
        decode_with_type_at(type_id, bytes, 0)
    }

    /// Returns the ZCL `TypeId` for this value.
    pub fn type_id(&self) -> TypeId {
        match self {
            Self::NoData => TypeId::NoData,
            Self::Bool(_) => TypeId::Boolean,
            Self::Uint8(_) => TypeId::Uint8,
            Self::Uint16(_) => TypeId::Uint16,
            Self::Uint24(_) => TypeId::Uint24,
            Self::Uint32(_) => TypeId::Uint32,
            Self::Uint40(_) => TypeId::Uint40,
            Self::Uint48(_) => TypeId::Uint48,
            Self::Uint56(_) => TypeId::Uint56,
            Self::Uint64(_) => TypeId::Uint64,
            Self::Int8(_) => TypeId::Int8,
            Self::Int16(_) => TypeId::Int16,
            Self::Int24(_) => TypeId::Int24,
            Self::Int32(_) => TypeId::Int32,
            Self::Int40(_) => TypeId::Int40,
            Self::Int48(_) => TypeId::Int48,
            Self::Int56(_) => TypeId::Int56,
            Self::Int64(_) => TypeId::Int64,
            Self::Data8(_) => TypeId::Data8,
            Self::Data16(_) => TypeId::Data16,
            Self::Data32(_) => TypeId::Data32,
            Self::Data64(_) => TypeId::Data64,
            Self::Bitmap8(_) => TypeId::Bitmap8,
            Self::Bitmap16(_) => TypeId::Bitmap16,
            Self::Bitmap32(_) => TypeId::Bitmap32,
            Self::Bitmap64(_) => TypeId::Bitmap64,
            Self::Enum8(_) => TypeId::Enum8,
            Self::Enum16(_) => TypeId::Enum16,
            Self::SemiFloat(_) => TypeId::SemiPrecision,
            Self::Float(_) => TypeId::SinglePrecision,
            Self::Double(_) => TypeId::DoublePrecision,
            Self::ShortText(_) => TypeId::CharacterString,
            Self::LongText(_) => TypeId::LongCharacterString,
            Self::ShortOctets(_) => TypeId::OctetString,
            Self::LongOctets(_) => TypeId::LongOctetString,
            Self::Array(_) => TypeId::Array,
            Self::Set(_) => TypeId::Set,
            Self::Bag(_) => TypeId::Bag,
            Self::Structure(_) => TypeId::Structure,
            Self::TimeOfDay(_) => TypeId::TimeOfDay,
            Self::Date(_) => TypeId::Date,
            Self::UtcTime(_) => TypeId::UtcTime,
            Self::ClusterId(_) => TypeId::ClusterId,
            Self::AttributeId(_) => TypeId::AttributeId,
            Self::BacnetOid(_) => TypeId::BacnetOid,
            Self::IeeeAddress(_) => TypeId::IeeeAddress,
            Self::SecurityKey(_) => TypeId::SecurityKey,
            Self::RawFixed(type_id, _) => *type_id,
        }
    }

    /// Returns the encoded byte count for this value (not including the type-id
    /// byte).
    pub fn encoded_len(&self) -> usize {
        match self {
            Self::NoData => 0,
            Self::Bool(_)
            | Self::Uint8(_)
            | Self::Int8(_)
            | Self::Data8(_)
            | Self::Bitmap8(_)
            | Self::Enum8(_)
            | Self::ShortText(None)
            | Self::ShortOctets(None) => 1,
            Self::Uint16(_)
            | Self::Int16(_)
            | Self::Data16(_)
            | Self::Bitmap16(_)
            | Self::Enum16(_)
            | Self::SemiFloat(_)
            | Self::ClusterId(_)
            | Self::AttributeId(_)
            | Self::LongText(None)
            | Self::LongOctets(None)
            | Self::Structure(MaybeStructRef::Null) => 2,
            Self::Uint24(_)
            | Self::Int24(_)
            | Self::Array(MaybeCollectionRef::Null { .. })
            | Self::Set(MaybeCollectionRef::Null { .. })
            | Self::Bag(MaybeCollectionRef::Null { .. }) => 3,
            Self::Uint32(_)
            | Self::Int32(_)
            | Self::Data32(_)
            | Self::Bitmap32(_)
            | Self::Float(_)
            | Self::TimeOfDay(_)
            | Self::Date(_)
            | Self::UtcTime(_)
            | Self::BacnetOid(_) => 4,
            Self::Uint40(_) | Self::Int40(_) => 5,
            Self::Uint48(_) | Self::Int48(_) => 6,
            Self::Uint56(_) | Self::Int56(_) => 7,
            Self::Uint64(_)
            | Self::Int64(_)
            | Self::Data64(_)
            | Self::Bitmap64(_)
            | Self::Double(_)
            | Self::IeeeAddress(_) => 8,
            Self::SecurityKey(_) => 16,
            Self::ShortText(Some(t)) => 1 + t.as_bytes().len(),
            Self::ShortOctets(Some(b)) => 1 + b.len(),
            Self::LongText(Some(t)) => 2 + t.as_bytes().len(),
            Self::LongOctets(Some(b)) => 2 + b.len(),
            Self::Array(MaybeCollectionRef::Some(c))
            | Self::Set(MaybeCollectionRef::Some(c))
            | Self::Bag(MaybeCollectionRef::Some(c)) => 3 + c.payload().len(),
            Self::Structure(MaybeStructRef::Some(s)) => 2 + s.payload().len(),
            Self::RawFixed(_, bytes) => bytes.len(),
        }
    }

    /// Encodes this value into `buf`, writing only the value bytes (not the
    /// type-id byte). Returns the number of bytes written.
    #[allow(clippy::too_many_lines)]
    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, ZclError> {
        #[inline]
        fn copy(buf: &mut [u8], src: &[u8]) -> Result<usize, ZclError> {
            let n = src.len();
            buf.get_mut(..n)
                .ok_or(ZclError::BufferTooSmall)?
                .copy_from_slice(src);
            Ok(n)
        }
        fn check_unsigned(value: u64, bits: u32) -> Result<(), ZclError> {
            let null = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            if value == null {
                return Err(ZclError::NullSentinel);
            }
            if value > null {
                return Err(ZclError::InvalidValue);
            }
            Ok(())
        }
        fn check_signed(value: i64, bits: u32) -> Result<(), ZclError> {
            let min = if bits == 64 {
                i64::MIN
            } else {
                -(1i64 << (bits - 1))
            };
            let max = if bits == 64 {
                i64::MAX
            } else {
                (1i64 << (bits - 1)) - 1
            };
            if value == min {
                return Err(ZclError::NullSentinel);
            }
            if value < min || value > max {
                return Err(ZclError::InvalidValue);
            }
            Ok(())
        }
        match self {
            Self::NoData => Ok(0),
            Self::Bool(v) => copy(buf, &[u8::from(*v)]),
            Self::Uint8(v) | Self::Enum8(v) => {
                check_unsigned(u64::from(*v), 8)?;
                copy(buf, &[*v])
            }
            Self::Data8(v) | Self::Bitmap8(v) => copy(buf, &[*v]),
            Self::Int8(v) => {
                check_signed(i64::from(*v), 8)?;
                copy(buf, &[v.cast_unsigned()])
            }
            Self::Uint16(v) | Self::Enum16(v) => {
                check_unsigned(u64::from(*v), 16)?;
                copy(buf, &v.to_le_bytes())
            }
            Self::Data16(v) | Self::Bitmap16(v) | Self::SemiFloat(v) => copy(buf, &v.to_le_bytes()),
            Self::Int16(v) => {
                check_signed(i64::from(*v), 16)?;
                copy(buf, &v.to_le_bytes())
            }
            Self::Uint24(v) => {
                check_unsigned(u64::from(*v), 24)?;
                copy(buf, &v.to_le_bytes()[..3])
            }
            Self::Int24(v) => {
                check_signed(i64::from(*v), 24)?;
                copy(buf, &v.to_le_bytes()[..3])
            }
            Self::Uint32(v) => {
                check_unsigned(u64::from(*v), 32)?;
                copy(buf, &v.to_le_bytes())
            }
            Self::Data32(v) | Self::Bitmap32(v) => copy(buf, &v.to_le_bytes()),
            Self::Int32(v) => {
                check_signed(i64::from(*v), 32)?;
                copy(buf, &v.to_le_bytes())
            }
            Self::Float(v) => {
                if v.is_nan() {
                    return Err(ZclError::NullSentinel);
                }
                copy(buf, &v.to_bits().to_le_bytes())
            }
            Self::TimeOfDay(v) => copy(buf, &v.0.to_le_bytes()),
            Self::Date(v) => copy(buf, &v.0.to_le_bytes()),
            Self::UtcTime(v) => copy(buf, &v.0.to_le_bytes()),
            Self::BacnetOid(v) => copy(buf, &v.0.to_le_bytes()),
            Self::Uint40(v) => {
                check_unsigned(*v, 40)?;
                copy(buf, &v.to_le_bytes()[..5])
            }
            Self::Int40(v) => {
                check_signed(*v, 40)?;
                copy(buf, &v.to_le_bytes()[..5])
            }
            Self::Uint48(v) => {
                check_unsigned(*v, 48)?;
                copy(buf, &v.to_le_bytes()[..6])
            }
            Self::Int48(v) => {
                check_signed(*v, 48)?;
                copy(buf, &v.to_le_bytes()[..6])
            }
            Self::Uint56(v) => {
                check_unsigned(*v, 56)?;
                copy(buf, &v.to_le_bytes()[..7])
            }
            Self::Int56(v) => {
                check_signed(*v, 56)?;
                copy(buf, &v.to_le_bytes()[..7])
            }
            Self::Uint64(v) => {
                check_unsigned(*v, 64)?;
                copy(buf, &v.to_le_bytes())
            }
            Self::Data64(v) | Self::Bitmap64(v) => copy(buf, &v.to_le_bytes()),
            Self::Int64(v) => {
                check_signed(*v, 64)?;
                copy(buf, &v.to_le_bytes())
            }
            Self::Double(v) => {
                if v.is_nan() {
                    return Err(ZclError::NullSentinel);
                }
                copy(buf, &v.to_bits().to_le_bytes())
            }
            Self::IeeeAddress(v) => copy(buf, &v.0.to_le_bytes()),
            Self::SecurityKey(k) => copy(buf, &k.0),
            Self::ClusterId(c) => copy(buf, &c.0.to_le_bytes()),
            Self::AttributeId(a) => copy(buf, &a.0.to_le_bytes()),
            Self::ShortText(None) | Self::ShortOctets(None) => copy(buf, &[0xFF]),
            Self::ShortText(Some(t)) => {
                let bytes = t.as_bytes();
                let len = u8::try_from(bytes.len()).map_err(|_| ZclError::InvalidLength)?;
                if len == 0xFF {
                    return Err(ZclError::InvalidLength);
                }
                let n = 1 + bytes.len();
                let dst = buf.get_mut(..n).ok_or(ZclError::BufferTooSmall)?;
                dst[0] = len;
                dst[1..].copy_from_slice(bytes);
                Ok(n)
            }
            Self::ShortOctets(Some(b)) => {
                let len = u8::try_from(b.len()).map_err(|_| ZclError::InvalidLength)?;
                if len == 0xFF {
                    return Err(ZclError::InvalidLength);
                }
                let n = 1 + b.len();
                let dst = buf.get_mut(..n).ok_or(ZclError::BufferTooSmall)?;
                dst[0] = len;
                dst[1..].copy_from_slice(b);
                Ok(n)
            }
            Self::LongText(None) | Self::LongOctets(None) => copy(buf, &[0xFF, 0xFF]),
            Self::LongText(Some(t)) => {
                let bytes = t.as_bytes();
                let len = u16::try_from(bytes.len()).map_err(|_| ZclError::InvalidLength)?;
                if len == 0xFFFF {
                    return Err(ZclError::InvalidLength);
                }
                let n = 2 + bytes.len();
                let dst = buf.get_mut(..n).ok_or(ZclError::BufferTooSmall)?;
                dst[..2].copy_from_slice(&len.to_le_bytes());
                dst[2..].copy_from_slice(bytes);
                Ok(n)
            }
            Self::LongOctets(Some(b)) => {
                let len = u16::try_from(b.len()).map_err(|_| ZclError::InvalidLength)?;
                if len == 0xFFFF {
                    return Err(ZclError::InvalidLength);
                }
                let n = 2 + b.len();
                let dst = buf.get_mut(..n).ok_or(ZclError::BufferTooSmall)?;
                dst[..2].copy_from_slice(&len.to_le_bytes());
                dst[2..].copy_from_slice(b);
                Ok(n)
            }
            Self::Array(v) | Self::Set(v) | Self::Bag(v) => match v {
                MaybeCollectionRef::Null { element_type, .. } => {
                    copy(buf, &[element_type.raw(), 0xFF, 0xFF])
                }
                MaybeCollectionRef::Some(c) => {
                    let payload = c.payload();
                    let n = 3 + payload.len();
                    let dst = buf.get_mut(..n).ok_or(ZclError::BufferTooSmall)?;
                    dst[0] = c.element_type().raw();
                    dst[1..3].copy_from_slice(&c.element_count().to_le_bytes());
                    dst[3..].copy_from_slice(payload);
                    Ok(n)
                }
            },
            Self::Structure(v) => match v {
                MaybeStructRef::Null => copy(buf, &[0xFF, 0xFF]),
                MaybeStructRef::Some(s) => {
                    let payload = s.payload();
                    let n = 2 + payload.len();
                    let dst = buf.get_mut(..n).ok_or(ZclError::BufferTooSmall)?;
                    dst[..2].copy_from_slice(&s.len().to_le_bytes());
                    dst[2..].copy_from_slice(payload);
                    Ok(n)
                }
            },
            Self::RawFixed(_, bytes) => copy(buf, bytes),
        }
    }

    fn collection(kind: CollectionKind, value: MaybeCollectionRef<'a>) -> Self {
        match kind {
            CollectionKind::Array => Self::Array(value),
            CollectionKind::Set => Self::Set(value),
            CollectionKind::Bag => Self::Bag(value),
        }
    }
}

// ZCL 2.6.2 spec caps structure nesting at 15 levels.
const MAX_DEPTH: u8 = 15;

fn decode_unsigned<'a, const N: usize, T>(
    bytes: &'a [u8],
    null: u64,
    convert: impl FnOnce(u64) -> T,
    wrap: impl FnOnce(T) -> ZclValueRef<'a>,
) -> Result<(ZclValueRef<'a>, usize), ZclError> {
    let raw = reject_null(read_uint::<N>(bytes)?, &null)?;
    Ok((wrap(convert(raw)), N))
}

fn decode_raw_unsigned<'a, const N: usize, T>(
    bytes: &'a [u8],
    convert: impl FnOnce(u64) -> T,
    wrap: impl FnOnce(T) -> ZclValueRef<'a>,
) -> Result<(ZclValueRef<'a>, usize), ZclError> {
    Ok((wrap(convert(read_uint::<N>(bytes)?)), N))
}

fn decode_signed<'a, const N: usize, T>(
    bytes: &'a [u8],
    null: u64,
    bits: u32,
    convert: impl FnOnce(i64) -> T,
    wrap: impl FnOnce(T) -> ZclValueRef<'a>,
) -> Result<(ZclValueRef<'a>, usize), ZclError> {
    let raw = reject_null(read_uint::<N>(bytes)?, &null)?;
    Ok((wrap(convert(sign_extend(raw, bits))), N))
}

fn reject_null<T: Eq>(value: T, null: &T) -> Result<T, ZclError> {
    if value == *null {
        Err(ZclError::NullSentinel)
    } else {
        Ok(value)
    }
}

fn decode_bool(byte: u8) -> Result<bool, ZclError> {
    match byte {
        0x00 => Ok(false),
        0x01 => Ok(true),
        0xFF => Err(ZclError::NullSentinel),
        _ => Err(ZclError::InvalidValue),
    }
}

fn decode_float32(bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    let value = f32::from_bits(u32::from_le_bytes(read_le::<4>(bytes)?));
    if value.is_nan() {
        Err(ZclError::NullSentinel)
    } else {
        Ok((ZclValueRef::Float(value), 4))
    }
}

fn decode_float64(bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    let value = f64::from_bits(u64::from_le_bytes(read_le::<8>(bytes)?));
    if value.is_nan() {
        Err(ZclError::NullSentinel)
    } else {
        Ok((ZclValueRef::Double(value), 8))
    }
}

fn decode_raw_fixed(type_id: TypeId, bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    let n = type_id.fixed_size().unwrap();
    let raw = bytes.get(..n).ok_or(ZclError::InsufficientBytes)?;
    Ok((ZclValueRef::RawFixed(type_id, raw), n))
}

fn collection_kind(type_id: TypeId) -> CollectionKind {
    match type_id {
        TypeId::Array => CollectionKind::Array,
        TypeId::Set => CollectionKind::Set,
        TypeId::Bag => CollectionKind::Bag,
        _ => unreachable!("non-collection type"),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn decode_with_type_at(
    type_id: TypeId,
    bytes: &[u8],
    depth: u8,
) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    if depth > MAX_DEPTH {
        return Err(ZclError::InvalidLength);
    }
    match type_id {
        TypeId::NoData => Ok((ZclValueRef::NoData, 0)),

        TypeId::Boolean => {
            let value = decode_bool(read_u8(bytes)?)?;
            Ok((ZclValueRef::Bool(value), 1))
        }

        TypeId::Uint8 => decode_unsigned::<1, u8>(bytes, 0xFF, |v| v as u8, ZclValueRef::Uint8),
        TypeId::Uint16 => {
            decode_unsigned::<2, u16>(bytes, 0xFFFF, |v| v as u16, ZclValueRef::Uint16)
        }
        TypeId::Uint24 => {
            decode_unsigned::<3, u32>(bytes, 0x00FF_FFFF, |v| v as u32, ZclValueRef::Uint24)
        }
        TypeId::Uint32 => {
            decode_unsigned::<4, u32>(bytes, 0xFFFF_FFFF, |v| v as u32, ZclValueRef::Uint32)
        }
        TypeId::Uint40 => {
            decode_unsigned::<5, u64>(bytes, 0xFF_FFFF_FFFF, |v| v, ZclValueRef::Uint40)
        }
        TypeId::Uint48 => {
            decode_unsigned::<6, u64>(bytes, 0xFFFF_FFFF_FFFF, |v| v, ZclValueRef::Uint48)
        }
        TypeId::Uint56 => {
            decode_unsigned::<7, u64>(bytes, 0xFF_FFFF_FFFF_FFFF, |v| v, ZclValueRef::Uint56)
        }
        TypeId::Uint64 => {
            decode_unsigned::<8, u64>(bytes, 0xFFFF_FFFF_FFFF_FFFF, |v| v, ZclValueRef::Uint64)
        }

        TypeId::Int8 => decode_signed::<1, i8>(bytes, 0x80, 8, |v| v as i8, ZclValueRef::Int8),
        TypeId::Int16 => {
            decode_signed::<2, i16>(bytes, 0x8000, 16, |v| v as i16, ZclValueRef::Int16)
        }
        TypeId::Int24 => {
            decode_signed::<3, i32>(bytes, 0x80_0000, 24, |v| v as i32, ZclValueRef::Int24)
        }
        TypeId::Int32 => {
            decode_signed::<4, i32>(bytes, 0x8000_0000, 32, |v| v as i32, ZclValueRef::Int32)
        }
        TypeId::Int40 => {
            decode_signed::<5, i64>(bytes, 0x80_0000_0000, 40, |v| v, ZclValueRef::Int40)
        }
        TypeId::Int48 => {
            decode_signed::<6, i64>(bytes, 0x8000_0000_0000, 48, |v| v, ZclValueRef::Int48)
        }
        TypeId::Int56 => {
            decode_signed::<7, i64>(bytes, 0x80_0000_0000_0000, 56, |v| v, ZclValueRef::Int56)
        }
        TypeId::Int64 => {
            decode_signed::<8, i64>(bytes, 0x8000_0000_0000_0000, 64, |v| v, ZclValueRef::Int64)
        }

        TypeId::Data8 => decode_raw_unsigned::<1, u8>(bytes, |v| v as u8, ZclValueRef::Data8),
        TypeId::Data16 => decode_raw_unsigned::<2, u16>(bytes, |v| v as u16, ZclValueRef::Data16),
        TypeId::Data32 => decode_raw_unsigned::<4, u32>(bytes, |v| v as u32, ZclValueRef::Data32),
        TypeId::Data64 => decode_raw_unsigned::<8, u64>(bytes, |v| v, ZclValueRef::Data64),
        TypeId::Data24
        | TypeId::Data40
        | TypeId::Data48
        | TypeId::Data56
        | TypeId::Bitmap24
        | TypeId::Bitmap40
        | TypeId::Bitmap48
        | TypeId::Bitmap56 => decode_raw_fixed(type_id, bytes),

        TypeId::Bitmap8 => decode_raw_unsigned::<1, u8>(bytes, |v| v as u8, ZclValueRef::Bitmap8),
        TypeId::Bitmap16 => {
            decode_raw_unsigned::<2, u16>(bytes, |v| v as u16, ZclValueRef::Bitmap16)
        }
        TypeId::Bitmap32 => {
            decode_raw_unsigned::<4, u32>(bytes, |v| v as u32, ZclValueRef::Bitmap32)
        }
        TypeId::Bitmap64 => decode_raw_unsigned::<8, u64>(bytes, |v| v, ZclValueRef::Bitmap64),

        TypeId::Enum8 => decode_unsigned::<1, u8>(bytes, 0xFF, |v| v as u8, ZclValueRef::Enum8),
        TypeId::Enum16 => {
            decode_unsigned::<2, u16>(bytes, 0xFFFF, |v| v as u16, ZclValueRef::Enum16)
        }

        TypeId::SemiPrecision => {
            decode_raw_unsigned::<2, u16>(bytes, |v| v as u16, ZclValueRef::SemiFloat)
        }
        TypeId::SinglePrecision => decode_float32(bytes),
        TypeId::DoublePrecision => decode_float64(bytes),

        TypeId::CharacterString => decode_short_text(bytes),
        TypeId::LongCharacterString => decode_long_text(bytes),
        TypeId::OctetString => decode_short_octets(bytes),
        TypeId::LongOctetString => decode_long_octets(bytes),

        TypeId::Array | TypeId::Set | TypeId::Bag => {
            decode_dynamic_collection(bytes, collection_kind(type_id), depth)
        }

        TypeId::Structure => decode_dynamic_struct(bytes, depth),

        TypeId::TimeOfDay => decode_raw_unsigned::<4, ZclTimeOfDay>(
            bytes,
            |v| ZclTimeOfDay(v as u32),
            ZclValueRef::TimeOfDay,
        ),
        TypeId::Date => {
            decode_raw_unsigned::<4, ZclDate>(bytes, |v| ZclDate(v as u32), ZclValueRef::Date)
        }
        TypeId::UtcTime => {
            decode_raw_unsigned::<4, UtcTime>(bytes, |v| UtcTime(v as u32), ZclValueRef::UtcTime)
        }

        TypeId::ClusterId => decode_raw_unsigned::<2, ClusterId>(
            bytes,
            |v| ClusterId(v as u16),
            ZclValueRef::ClusterId,
        ),
        TypeId::AttributeId => decode_raw_unsigned::<2, AttributeId>(
            bytes,
            |v| AttributeId(v as u16),
            ZclValueRef::AttributeId,
        ),
        TypeId::BacnetOid => decode_raw_unsigned::<4, BacnetOid>(
            bytes,
            |v| BacnetOid(v as u32),
            ZclValueRef::BacnetOid,
        ),

        TypeId::IeeeAddress => {
            decode_raw_unsigned::<8, IeeeAddress>(bytes, IeeeAddress, ZclValueRef::IeeeAddress)
        }
        TypeId::SecurityKey => {
            let raw = bytes
                .get(..16)
                .ok_or(ZclError::InsufficientBytes)?
                .try_into()
                .map_err(|_| ZclError::InsufficientBytes)?;
            Ok((ZclValueRef::SecurityKey(SecurityKey(raw)), 16))
        }

        TypeId::Unknown => Err(ZclError::InvalidValue),
    }
}

fn read_le<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ZclError> {
    let mut out = [0u8; N];
    out.copy_from_slice(bytes.get(..N).ok_or(ZclError::InsufficientBytes)?);
    Ok(out)
}

fn read_u8(bytes: &[u8]) -> Result<u8, ZclError> {
    bytes.first().copied().ok_or(ZclError::InsufficientBytes)
}

fn read_uint<const N: usize>(bytes: &[u8]) -> Result<u64, ZclError> {
    let src = read_le::<N>(bytes)?;
    let mut raw = [0u8; 8];
    raw[..N].copy_from_slice(&src);
    Ok(u64::from_le_bytes(raw))
}

fn sign_extend(value: u64, bits: u32) -> i64 {
    let shift = 64 - bits;
    (value.cast_signed() << shift) >> shift
}

fn decode_len_prefixed<const N: usize>(
    bytes: &[u8],
    null_len: u64,
) -> Result<Option<(&[u8], usize)>, ZclError> {
    let len = read_uint::<N>(bytes)?;
    if len == null_len {
        return Ok(None);
    }

    let len = usize::try_from(len).map_err(|_| ZclError::InvalidLength)?;
    let end = N.checked_add(len).ok_or(ZclError::InvalidLength)?;
    let payload = bytes.get(N..end).ok_or(ZclError::InsufficientBytes)?;
    Ok(Some((payload, end)))
}

fn decode_short_text(bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    match decode_len_prefixed::<1>(bytes, 0xFF)? {
        None => Ok((ZclValueRef::ShortText(None), 1)),
        Some((payload, used)) => Ok((ZclValueRef::ShortText(Some(ZclText::new(payload))), used)),
    }
}

fn decode_long_text(bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    match decode_len_prefixed::<2>(bytes, 0xFFFF)? {
        None => Ok((ZclValueRef::LongText(None), 2)),
        Some((payload, used)) => Ok((ZclValueRef::LongText(Some(ZclText::new(payload))), used)),
    }
}

fn decode_short_octets(bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    match decode_len_prefixed::<1>(bytes, 0xFF)? {
        None => Ok((ZclValueRef::ShortOctets(None), 1)),
        Some((payload, used)) => Ok((ZclValueRef::ShortOctets(Some(payload)), used)),
    }
}

fn decode_long_octets(bytes: &[u8]) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    match decode_len_prefixed::<2>(bytes, 0xFFFF)? {
        None => Ok((ZclValueRef::LongOctets(None), 2)),
        Some((payload, used)) => Ok((ZclValueRef::LongOctets(Some(payload)), used)),
    }
}

fn decode_dynamic_collection(
    bytes: &[u8],
    kind: CollectionKind,
    depth: u8,
) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    if bytes.len() < 3 {
        return Err(ZclError::InsufficientBytes);
    }
    let element_type = RawTypeId::new(bytes[0]);
    let element_count = u16::from_le_bytes([bytes[1], bytes[2]]);

    if element_count == 0xFFFF {
        let value = MaybeCollectionRef::Null { kind, element_type };
        return Ok((ZclValueRef::collection(kind, value), 3));
    }

    let payload_start = 3usize;
    let payload_bytes = bytes
        .get(payload_start..)
        .ok_or(ZclError::InsufficientBytes)?;
    let payload_len = scan_elements(element_type, payload_bytes, element_count, depth)?;
    let payload = &payload_bytes[..payload_len];
    let collection = ZclCollectionRef::new(kind, element_type, element_count, payload);
    Ok((
        ZclValueRef::collection(kind, MaybeCollectionRef::Some(collection)),
        payload_start + payload_len,
    ))
}

fn decode_dynamic_struct(bytes: &[u8], depth: u8) -> Result<(ZclValueRef<'_>, usize), ZclError> {
    if bytes.len() < 2 {
        return Err(ZclError::InsufficientBytes);
    }
    let field_count = u16::from_le_bytes([bytes[0], bytes[1]]);
    if field_count == 0xFFFF {
        return Ok((ZclValueRef::Structure(MaybeStructRef::Null), 2));
    }
    let mut offset = 2usize;
    for _ in 0..field_count {
        let field_type_byte = bytes
            .get(offset)
            .copied()
            .ok_or(ZclError::InsufficientBytes)?;
        let field_type = RawTypeId::new(field_type_byte);
        let Some(known_field_type) = field_type.known() else {
            return Err(ZclError::InvalidValue);
        };
        offset += 1;
        let remaining = bytes.get(offset..).ok_or(ZclError::InsufficientBytes)?;
        let (_, used) = decode_with_type_at(known_field_type, remaining, depth + 1)?;
        offset += used;
    }
    let payload = bytes.get(2..offset).ok_or(ZclError::InsufficientBytes)?;
    let struct_ref = ZclStructRef::new(field_count, payload);
    Ok((
        ZclValueRef::Structure(MaybeStructRef::Some(struct_ref)),
        offset,
    ))
}

fn scan_elements(
    element_type: RawTypeId,
    payload: &[u8],
    count: u16,
    depth: u8,
) -> Result<usize, ZclError> {
    if count == 0 {
        return Ok(0);
    }
    let Some(element_type) = element_type.known() else {
        return Err(ZclError::InvalidValue);
    };
    if let Some(fixed) = element_type.fixed_size() {
        let total = (count as usize)
            .checked_mul(fixed)
            .ok_or(ZclError::InvalidLength)?;
        if payload.len() < total {
            return Err(ZclError::InsufficientBytes);
        }
        if !element_type.all_patterns_valid() {
            let mut offset = 0;
            for _ in 0..count {
                let (_, used) = decode_with_type_at(element_type, &payload[offset..], depth + 1)?;
                debug_assert_eq!(used, fixed);
                offset += fixed;
            }
        }
        Ok(total)
    } else {
        let mut offset = 0;
        for _ in 0..count {
            let remaining = payload.get(offset..).ok_or(ZclError::InsufficientBytes)?;
            if remaining.is_empty() {
                return Err(ZclError::InsufficientBytes);
            }
            let (_, used) = decode_with_type_at(element_type, remaining, depth + 1)?;
            offset += used;
        }
        Ok(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_roundtrip_uint8() {
        let bytes = [0x42u8];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Uint8, &bytes).unwrap();
        assert_eq!(n, 1);
        assert_eq!(v, ZclValueRef::Uint8(0x42));
    }

    #[test]
    fn scalar_roundtrip_int16() {
        let bytes = (-100i16).to_le_bytes();
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Int16, &bytes).unwrap();
        assert_eq!(n, 2);
        assert_eq!(v, ZclValueRef::Int16(-100));
    }

    #[test]
    fn scalar_roundtrip_bool() {
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Boolean, &[0x01]).unwrap();
        assert_eq!(n, 1);
        assert_eq!(v, ZclValueRef::Bool(true));

        assert!(ZclValueRef::decode_with_type(TypeId::Boolean, &[0x02]).is_err());
    }

    #[test]
    fn short_text_null() {
        let (v, n) = ZclValueRef::decode_with_type(TypeId::CharacterString, &[0xFF]).unwrap();
        assert_eq!(n, 1);
        assert_eq!(v, ZclValueRef::ShortText(None));
    }

    #[test]
    fn short_text_value() {
        let bytes = [3u8, b'H', b'i', b'!'];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::CharacterString, &bytes).unwrap();
        assert_eq!(n, 4);
        if let ZclValueRef::ShortText(Some(t)) = v {
            assert_eq!(t.as_bytes(), b"Hi!");
        } else {
            panic!("expected ShortText(Some)");
        }
    }

    #[test]
    fn uint24_decode() {
        let bytes = [0x56u8, 0x34, 0x12];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Uint24, &bytes).unwrap();
        assert_eq!(n, 3);
        assert_eq!(v, ZclValueRef::Uint24(0x12_3456));
    }

    #[test]
    fn int24_sign_extend() {
        // -1 in 24-bit = 0xFFFFFF
        let bytes = [0xFFu8, 0xFF, 0xFF];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Int24, &bytes).unwrap();
        assert_eq!(n, 3);
        assert_eq!(v, ZclValueRef::Int24(-1));
    }
    #[test]
    fn raw_fixed_type_preserves_bytes() {
        let bytes = [0x56u8, 0x34, 0x12];
        let (value, used) = ZclValueRef::decode_with_type(TypeId::Data24, &bytes).unwrap();

        assert_eq!(used, bytes.len());
        assert_eq!(value, ZclValueRef::RawFixed(TypeId::Data24, &bytes));
    }

    #[test]
    fn unknown_type_id_is_not_raw_fixed() {
        assert_eq!(
            ZclValueRef::decode_with_type(TypeId::Unknown, &[0x00]).unwrap_err(),
            ZclError::InvalidValue
        );
    }

    #[test]
    fn array_null() {
        let bytes = [0x21u8, 0xFF, 0xFF];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Array, &bytes).unwrap();
        assert_eq!(n, 3);
        assert_eq!(
            v,
            ZclValueRef::Array(MaybeCollectionRef::Null {
                kind: CollectionKind::Array,
                element_type: RawTypeId::from_type_id(TypeId::Uint16),
            })
        );
    }

    #[test]
    fn structure_null() {
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Structure, &[0xFF, 0xFF]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(v, ZclValueRef::Structure(MaybeStructRef::Null));
    }

    #[test]
    fn array_fixed_elements() {
        // 2 Uint16 elements: [0x0100, 0x0200]
        let bytes = [0x21u8, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::Array, &bytes).unwrap();
        assert_eq!(n, 7);
        if let ZclValueRef::Array(MaybeCollectionRef::Some(c)) = v {
            assert_eq!(c.element_count(), 2);
            assert_eq!(c.element_type(), TypeId::Uint16);
        } else {
            panic!("expected Array(MaybeCollectionRef::Some)");
        }
    }

    #[test]
    fn array_fixed_restricted_elements_reject_invalid_values() {
        let invalid_bool = [0x10u8, 0x01, 0x00, 0x02];
        assert_eq!(
            ZclValueRef::decode_with_type(TypeId::Array, &invalid_bool).unwrap_err(),
            ZclError::InvalidValue
        );

        // 0xFF is the ZCL null sentinel for Boolean, not just an invalid value.
        let null_bool = [0x10u8, 0x01, 0x00, 0xFF];
        assert_eq!(
            ZclValueRef::decode_with_type(TypeId::Array, &null_bool).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn numeric_null_sentinel_rejected_dynamically() {
        assert_eq!(
            ZclValueRef::decode_with_type(TypeId::Uint16, &[0xFF, 0xFF]).unwrap_err(),
            ZclError::NullSentinel
        );

        let array_with_null_u16 = [0x21u8, 0x01, 0x00, 0xFF, 0xFF];
        assert_eq!(
            ZclValueRef::decode_with_type(TypeId::Array, &array_with_null_u16).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn nullable_collection_preserves_unknown_element_type_raw_byte() {
        let (value, used) =
            ZclValueRef::decode_with_type(TypeId::Array, &[0xFE, 0xFF, 0xFF]).unwrap();
        assert_eq!(used, 3);
        assert_eq!(
            value,
            ZclValueRef::Array(MaybeCollectionRef::Null {
                kind: CollectionKind::Array,
                element_type: RawTypeId::new(0xFE),
            })
        );
    }

    #[test]
    fn empty_collection_preserves_unknown_element_type_raw_byte() {
        let (value, used) =
            ZclValueRef::decode_with_type(TypeId::Array, &[0xFE, 0x00, 0x00]).unwrap();
        assert_eq!(used, 3);
        assert_eq!(
            value,
            ZclValueRef::Array(MaybeCollectionRef::Some(ZclCollectionRef::new(
                CollectionKind::Array,
                RawTypeId::new(0xFE),
                0,
                &[],
            )))
        );
    }

    #[test]
    fn ieee_address_decode() {
        let bytes: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let (v, n) = ZclValueRef::decode_with_type(TypeId::IeeeAddress, &bytes).unwrap();
        assert_eq!(n, 8);
        assert_eq!(
            v,
            ZclValueRef::IeeeAddress(IeeeAddress(0x0807_0605_0403_0201))
        );
    }

    #[test]
    fn encode_rejects_numeric_null_sentinels() {
        let mut buf = [0u8; 8];
        assert_eq!(
            ZclValueRef::Uint8(0xFF).encode(&mut buf),
            Err(ZclError::NullSentinel)
        );
        assert_eq!(
            ZclValueRef::Enum16(0xFFFF).encode(&mut buf),
            Err(ZclError::NullSentinel)
        );
        assert_eq!(
            ZclValueRef::Float(f32::NAN).encode(&mut buf),
            Err(ZclError::NullSentinel)
        );
    }

    #[test]
    fn encode_rejects_subwidth_integer_overflow() {
        let mut buf = [0u8; 8];
        assert_eq!(
            ZclValueRef::Uint24(0x01_000000).encode(&mut buf),
            Err(ZclError::InvalidValue)
        );
        assert_eq!(
            ZclValueRef::Int24(0x0080_0000).encode(&mut buf),
            Err(ZclError::InvalidValue)
        );
    }

    #[test]
    fn encode_rejects_string_lengths_reserved_for_null() {
        let short = [b'a'; 255];
        let long = [b'a'; 65_535];
        let mut buf = [0u8; 4];
        assert_eq!(
            ZclValueRef::ShortText(Some(ZclText::new(&short))).encode(&mut buf),
            Err(ZclError::InvalidLength)
        );
        assert_eq!(
            ZclValueRef::LongOctets(Some(&long)).encode(&mut buf),
            Err(ZclError::InvalidLength)
        );
    }
}
