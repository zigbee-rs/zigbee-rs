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
}
