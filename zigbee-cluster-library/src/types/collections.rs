use core::marker::PhantomData;

use super::error::ZclError;
use super::ids::RawTypeId;
use super::ids::TypeId;
use super::nullable::ZclHasNull;
use super::schema::ZclSchema;
use super::value::ZclValueRef;

/// Compile-time kind marker for ZCL collections (Array, Set, Bag).
pub trait CollectionKindTypestate {
    const TYPE_ID: TypeId;
    /// Called after element validation to enforce kind-specific constraints.
    /// Default: no-op. `Set<RawUniqueSet>` overrides to reject duplicate
    /// elements.
    fn validate_uniqueness<S: ZclSchema>(payload: &[u8], count: u16) -> Result<(), ZclError> {
        let _ = (payload, count);
        Ok(())
    }
}

/// Marker for ZCL Array collections.
pub enum Array {}
/// Marker for ZCL Bag collections (duplicates allowed, unordered).
pub enum Bag {}

/// Marker trait for set element uniqueness policies.
pub trait SetPolicy {
    fn validate<S: ZclSchema>(payload: &[u8], count: u16) -> Result<(), ZclError> {
        let _ = (payload, count);
        Ok(())
    }
}

/// Set policy: no duplicate checking (fast, no-alloc). Default.
pub enum UncheckedSet {}
/// Set policy: O(n^2) pairwise raw-slice comparison, no allocation.
pub enum RawUniqueSet {}

impl SetPolicy for UncheckedSet {}
impl SetPolicy for RawUniqueSet {
    fn validate<S: ZclSchema>(payload: &[u8], count: u16) -> Result<(), ZclError> {
        validate_set_uniqueness::<S>(payload, count)
    }
}

/// Marker for ZCL Set collections, parameterized by uniqueness policy.
pub struct Set<P: SetPolicy = UncheckedSet>(PhantomData<P>);

impl CollectionKindTypestate for Array {
    const TYPE_ID: TypeId = TypeId::Array;
}
impl CollectionKindTypestate for Bag {
    const TYPE_ID: TypeId = TypeId::Bag;
}
impl<P: SetPolicy> CollectionKindTypestate for Set<P> {
    const TYPE_ID: TypeId = TypeId::Set;

    fn validate_uniqueness<S: ZclSchema>(payload: &[u8], count: u16) -> Result<(), ZclError> {
        P::validate::<S>(payload, count)
    }
}

/// Validated ZCL collection element count. Rejects the null sentinel (0xFFFF).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionCount(u16);

impl CollectionCount {
    pub fn new(raw: u16) -> Result<Self, ZclError> {
        if raw == 0xFFFF {
            Err(ZclError::NullSentinel)
        } else {
            Ok(Self(raw))
        }
    }
    pub const fn get(self) -> u16 {
        self.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Borrowed, validated view of a typed ZCL collection.
///
/// `K` carries the collection kind (Array/Set/Bag) at the type level — no
/// runtime kind byte stored. `S` is the element schema. Only constructible
/// from `CollectionOf<K, S>::decode`.
pub struct CollectionRef<'a, K, S> {
    count: CollectionCount,
    payload: &'a [u8],
    _kind: PhantomData<K>,
    _schema: PhantomData<S>,
}

/// Typed view of a ZCL Array.
pub type ArrayRef<'a, S> = CollectionRef<'a, Array, S>;
/// Typed view of a ZCL Set (default: unchecked uniqueness).
pub type SetRef<'a, S, P = UncheckedSet> = CollectionRef<'a, Set<P>, S>;
/// Typed view of a ZCL Bag.
pub type BagRef<'a, S> = CollectionRef<'a, Bag, S>;

impl<K, S> Clone for CollectionRef<'_, K, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<K, S> Copy for CollectionRef<'_, K, S> {}

impl<K, S> core::fmt::Debug for CollectionRef<'_, K, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CollectionRef")
            .field("count", &self.count.get())
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

impl<'a, K, S: ZclSchema> CollectionRef<'a, K, S> {
    pub const fn len(&self) -> u16 {
        self.count.get()
    }
    pub const fn is_empty(&self) -> bool {
        self.count.is_empty()
    }
    pub const fn iter(&self) -> CollectionIter<'a, S> {
        CollectionIter(self.decoder())
    }
    pub const fn decoder(&self) -> CollectionDecoder<'a, S> {
        CollectionDecoder {
            remaining: self.count.get(),
            cursor: self.payload,
            _schema: PhantomData,
        }
    }
}

/// Single-pass decoder over a typed ZCL collection payload.
pub struct CollectionDecoder<'a, S> {
    remaining: u16,
    cursor: &'a [u8],
    _schema: PhantomData<S>,
}

impl<'a, S: ZclSchema + 'a> CollectionDecoder<'a, S> {
    pub fn finish(self) -> Result<(), ZclError> {
        if self.remaining == 0 && self.cursor.is_empty() {
            Ok(())
        } else {
            Err(ZclError::UnconsumedData)
        }
    }
}

impl<'a, S: ZclSchema + 'a> Iterator for CollectionDecoder<'a, S> {
    type Item = Result<S::Value<'a>, ZclError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        match decode_schema_value::<S>(self.cursor) {
            Err(e) => {
                self.remaining = 0;
                Some(Err(e))
            }
            Ok((value, used)) => match self.cursor.get(used..) {
                None => Some(Err(ZclError::InsufficientBytes)),
                Some(rest) => {
                    self.cursor = rest;
                    self.remaining -= 1;
                    Some(Ok(value))
                }
            },
        }
    }
}

/// Iterator over a typed ZCL collection. Yields decoded element values.
pub struct CollectionIter<'a, S>(CollectionDecoder<'a, S>);

impl<'a, S: ZclSchema + 'a> Iterator for CollectionIter<'a, S> {
    type Item = Result<S::Value<'a>, ZclError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, K, S: ZclSchema + 'a> IntoIterator for &CollectionRef<'a, K, S> {
    type Item = Result<S::Value<'a>, ZclError>;
    type IntoIter = CollectionIter<'a, S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Unified ZCL collection schema. Kind `K` selects Array, Set, or Bag at the
/// type level; element schema `S` controls the wire element type and codec.
pub struct CollectionOf<K, S>(PhantomData<(K, S)>);

/// Schema for a ZCL Array of elements `S`.
pub type ArrayOf<S> = CollectionOf<Array, S>;
/// Schema for a ZCL Set of elements `S` with uniqueness policy `P`.
pub type SetOf<S, P = UncheckedSet> = CollectionOf<Set<P>, S>;
/// Schema for a ZCL Bag of elements `S` (duplicates allowed, unordered).
pub type BagOf<S> = CollectionOf<Bag, S>;

fn decode_collection<K, S>(
    bytes: &[u8],
    reject_trailing: bool,
) -> Result<(CollectionRef<'_, K, S>, usize), ZclError>
where
    K: CollectionKindTypestate,
    S: ZclSchema,
{
    if bytes.len() < 3 {
        return Err(ZclError::InsufficientBytes);
    }
    let element_type = TypeId::from_u8(bytes[0]);
    let count_raw = u16::from_le_bytes([bytes[1], bytes[2]]);
    let count = CollectionCount::new(count_raw)?;

    if element_type != S::TYPE_ID {
        return Err(ZclError::TypeIdMismatch {
            expected: S::TYPE_ID,
            found: element_type,
        });
    }

    let payload_bytes = bytes.get(3..).ok_or(ZclError::InsufficientBytes)?;
    let payload_len = compute_payload_len::<S>(payload_bytes, count.get())?;
    let used = 3 + payload_len;

    if reject_trailing && used != bytes.len() {
        return Err(ZclError::InvalidLength);
    }

    let payload = &payload_bytes[..payload_len];

    K::validate_uniqueness::<S>(payload, count.get())?;

    Ok((
        CollectionRef {
            count,
            payload,
            _kind: PhantomData,
            _schema: PhantomData,
        },
        used,
    ))
}

impl<K, S> ZclSchema for CollectionOf<K, S>
where
    K: CollectionKindTypestate,
    S: ZclSchema,
{
    type Value<'a>
        = CollectionRef<'a, K, S>
    where
        S: 'a,
        K: 'a;
    const TYPE_ID: TypeId = K::TYPE_ID;
    const ENCODED_SIZE: Option<usize> = None;

    fn decode(bytes: &[u8]) -> Result<(CollectionRef<'_, K, S>, usize), ZclError> {
        decode_collection::<K, S>(bytes, true)
    }

    fn decode_prefix(bytes: &[u8]) -> Result<(CollectionRef<'_, K, S>, usize), ZclError> {
        decode_collection::<K, S>(bytes, false)
    }

    fn encode(value: CollectionRef<'_, K, S>, bytes: &mut [u8]) -> Result<usize, ZclError> {
        let count = value.count.get();
        let payload = value.payload;
        let total = 3 + payload.len();
        if bytes.len() < total {
            return Err(ZclError::BufferTooSmall);
        }
        bytes[0] = S::TYPE_ID.as_u8();
        bytes[1..3].copy_from_slice(&count.to_le_bytes());
        bytes[3..total].copy_from_slice(payload);
        Ok(total)
    }
}

impl<K, S> ZclHasNull for CollectionOf<K, S>
where
    K: CollectionKindTypestate,
    S: ZclSchema,
{
    fn null_size(bytes: &[u8]) -> Option<usize> {
        (bytes.first() == Some(&S::TYPE_ID.as_u8()) && bytes.get(1..3) == Some(&[0xFF, 0xFF]))
            .then_some(3)
    }

    fn encode_null(buf: &mut [u8]) -> Result<usize, ZclError> {
        if buf.len() < 3 {
            return Err(ZclError::BufferTooSmall);
        }
        buf[0] = S::TYPE_ID.as_u8();
        buf[1..3].copy_from_slice(&[0xFF, 0xFF]);
        Ok(3)
    }
}

/// Streaming encoder for ZCL collection payloads (Array, Set, or Bag).
///
/// `new()` writes the element `TypeId` and reserves two bytes for the count.
/// `push()` appends one encoded element. `finish()` commits the count and
/// returns total bytes written. Works for Copy scalars, borrowed strings,
/// nested structures, and nested collections without allocation.
pub struct CollectionEncoder<'a, K, S> {
    buf: &'a mut [u8],
    offset: usize,
    count: u16,
    _kind: PhantomData<K>,
    _schema: PhantomData<S>,
}

impl<K, S> core::fmt::Debug for CollectionEncoder<'_, K, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CollectionEncoder")
            .field("offset", &self.offset)
            .field("count", &self.count)
            .finish()
    }
}

impl<'a, K: CollectionKindTypestate, S: ZclSchema> CollectionEncoder<'a, K, S> {
    pub fn new(buf: &'a mut [u8]) -> Result<Self, ZclError> {
        if buf.len() < 3 {
            return Err(ZclError::BufferTooSmall);
        }
        buf[0] = S::TYPE_ID.as_u8();
        Ok(Self {
            buf,
            offset: 3,
            count: 0,
            _kind: PhantomData,
            _schema: PhantomData,
        })
    }

    pub fn push(&mut self, value: S::Value<'_>) -> Result<(), ZclError> {
        if self.count >= 0xFFFE {
            return Err(ZclError::InvalidLength);
        }
        let available = self
            .buf
            .get_mut(self.offset..)
            .ok_or(ZclError::BufferTooSmall)?;
        let written = S::encode(value, available)?;
        self.offset += written;
        self.count += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<usize, ZclError> {
        K::validate_uniqueness::<S>(&self.buf[3..self.offset], self.count)?;
        self.buf[1..3].copy_from_slice(&self.count.to_le_bytes());
        Ok(self.offset)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionKind {
    Array,
    Set,
    Bag,
}

/// Dynamic (schema-unknown) view of a non-null ZCL collection.
/// Used by gateways, bridges, and logging tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZclCollectionRef<'a> {
    kind: CollectionKind,
    element_type: RawTypeId,
    element_count: u16,
    payload: &'a [u8],
}

impl<'a> ZclCollectionRef<'a> {
    pub const fn new(
        kind: CollectionKind,
        element_type: RawTypeId,
        element_count: u16,
        payload: &'a [u8],
    ) -> Self {
        Self {
            kind,
            element_type,
            element_count,
            payload,
        }
    }

    pub const fn kind(&self) -> CollectionKind {
        self.kind
    }
    pub const fn element_type(&self) -> RawTypeId {
        self.element_type
    }
    pub const fn element_count(&self) -> u16 {
        self.element_count
    }
    pub const fn is_empty(&self) -> bool {
        self.element_count == 0
    }
    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }
    pub const fn iter(&self) -> ZclCollectionIter<'a> {
        ZclCollectionIter {
            element_type: self.element_type,
            payload: self.payload,
            offset: 0,
            remaining: self.element_count,
        }
    }
}

impl<'a> IntoIterator for &ZclCollectionRef<'a> {
    type Item = Result<ZclValueRef<'a>, ZclError>;
    type IntoIter = ZclCollectionIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Dynamic ZCL collection with null-preservation.
///
/// The `Null` variant retains element type and kind so the null sentinel can
/// be re-encoded faithfully — required for gateway and bridge forwarding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaybeCollectionRef<'a> {
    /// Null sentinel (count == 0xFFFF). Element type and kind preserved.
    Null {
        kind: CollectionKind,
        element_type: RawTypeId,
    },
    Some(ZclCollectionRef<'a>),
}

/// Dynamic ZCL structure with null-preservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaybeStructRef<'a> {
    /// Null sentinel (field count == 0xFFFF).
    Null,
    Some(ZclStructRef<'a>),
}

/// Iterator over a dynamic ZCL collection.
pub struct ZclCollectionIter<'a> {
    element_type: RawTypeId,
    payload: &'a [u8],
    offset: usize,
    remaining: u16,
}

impl<'a> Iterator for ZclCollectionIter<'a> {
    type Item = Result<ZclValueRef<'a>, ZclError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let Some(bytes) = self.payload.get(self.offset..) else {
            self.remaining = 0;
            return Some(Err(ZclError::InsufficientBytes));
        };
        let Some(element_type) = self.element_type.known() else {
            self.remaining = 0;
            return Some(Err(ZclError::InvalidValue));
        };
        match ZclValueRef::decode_with_type(element_type, bytes) {
            Ok((value, used)) => {
                self.offset += used;
                self.remaining -= 1;
                Some(Ok(value))
            }
            Err(e) => {
                self.remaining = 0;
                Some(Err(e))
            }
        }
    }
}

/// Dynamic (schema-unknown) view of a ZCL structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZclStructRef<'a> {
    field_count: u16,
    payload: &'a [u8],
}

impl<'a> ZclStructRef<'a> {
    pub const fn new(field_count: u16, payload: &'a [u8]) -> Self {
        Self {
            field_count,
            payload,
        }
    }

    pub const fn len(&self) -> u16 {
        self.field_count
    }
    pub const fn is_empty(&self) -> bool {
        self.field_count == 0
    }
    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }
    pub const fn fields(&self) -> ZclStructFields<'a> {
        ZclStructFields {
            payload: self.payload,
            offset: 0,
            remaining: self.field_count,
        }
    }
}

/// One dynamically decoded ZCL structure field.
#[derive(Debug, PartialEq)]
pub struct ZclFieldRef<'a> {
    pub type_id: RawTypeId,
    pub value: ZclValueRef<'a>,
}

/// Iterator over dynamically decoded ZCL structure fields.
pub struct ZclStructFields<'a> {
    payload: &'a [u8],
    offset: usize,
    remaining: u16,
}

impl<'a> Iterator for ZclStructFields<'a> {
    type Item = Result<ZclFieldRef<'a>, ZclError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let Some(type_byte) = self.payload.get(self.offset).copied() else {
            self.remaining = 0;
            return Some(Err(ZclError::InsufficientBytes));
        };
        let type_id = RawTypeId::new(type_byte);
        self.offset += 1;

        let Some(bytes) = self.payload.get(self.offset..) else {
            self.remaining = 0;
            return Some(Err(ZclError::InsufficientBytes));
        };
        let Some(known_type_id) = type_id.known() else {
            self.remaining = 0;
            return Some(Err(ZclError::InvalidValue));
        };

        match ZclValueRef::decode_with_type(known_type_id, bytes) {
            Ok((value, used)) => {
                self.offset += used;
                self.remaining -= 1;
                Some(Ok(ZclFieldRef { type_id, value }))
            }
            Err(e) => {
                self.remaining = 0;
                Some(Err(e))
            }
        }
    }
}

/// Typed structure schema. Implement to define composite ZCL structures.
///
/// Use `StructOf<T>` to make a `ZclStructSchema` usable as a full `ZclSchema`
/// value, enabling `ArrayOf<StructOf<T>>`, `Nullable<StructOf<T>>`, etc.
pub trait ZclStructSchema {
    type Value<'a>
    where
        Self: 'a;
    fn decode_fields<'a>(decoder: &mut StructDecoder<'a>) -> Result<Self::Value<'a>, ZclError>;
    fn encode_fields(
        value: Self::Value<'_>,
        encoder: &mut StructEncoder<'_>,
    ) -> Result<(), ZclError>;
}

/// Streaming decoder for ZCL structure fields.
#[derive(Debug)]
pub struct StructDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: u16,
}

impl<'a> StructDecoder<'a> {
    /// Read the `field_count` header and construct the decoder.
    /// Returns `Err(ZclError::NullSentinel)` if `field_count` == 0xFFFF.
    pub fn new(bytes: &'a [u8]) -> Result<(Self, usize), ZclError> {
        if bytes.len() < 2 {
            return Err(ZclError::InsufficientBytes);
        }
        let field_count = u16::from_le_bytes([bytes[0], bytes[1]]);
        if field_count == 0xFFFF {
            return Err(ZclError::NullSentinel);
        }
        Ok((
            StructDecoder {
                bytes,
                offset: 2,
                remaining: field_count,
            },
            2,
        ))
    }

    /// Decode the next field, validating wire `TypeId` against `S::TYPE_ID`.
    pub fn field<S: ZclSchema>(&mut self) -> Result<S::Value<'a>, ZclError> {
        if self.remaining == 0 {
            return Err(ZclError::UnconsumedData);
        }
        let wire_byte = self
            .bytes
            .get(self.offset)
            .copied()
            .ok_or(ZclError::InsufficientBytes)?;
        let wire_type = TypeId::from_u8(wire_byte);
        if wire_type != S::TYPE_ID {
            return Err(ZclError::TypeIdMismatch {
                expected: S::TYPE_ID,
                found: wire_type,
            });
        }
        self.offset += 1;
        let remaining = self
            .bytes
            .get(self.offset..)
            .ok_or(ZclError::InsufficientBytes)?;
        let (value, used) = decode_schema_value::<S>(remaining)?;
        self.offset += used;
        self.remaining -= 1;
        Ok(value)
    }

    /// Verify all declared fields were consumed.
    pub fn finish(self) -> Result<(), ZclError> {
        if self.remaining != 0 {
            return Err(ZclError::UnconsumedData);
        }
        Ok(())
    }

    pub fn bytes_consumed(&self) -> usize {
        self.offset
    }
}

/// Streaming encoder for ZCL structure fields.
pub struct StructEncoder<'a> {
    bytes: &'a mut [u8],
    offset: usize,
    field_count: u16,
}

impl<'a> StructEncoder<'a> {
    /// Reserve space for the `field_count` header (written by `finish`).
    pub fn new(bytes: &'a mut [u8]) -> Result<Self, ZclError> {
        if bytes.len() < 2 {
            return Err(ZclError::BufferTooSmall);
        }
        Ok(StructEncoder {
            bytes,
            offset: 2,
            field_count: 0,
        })
    }

    /// Write one field: `TypeId` byte followed by encoded value.
    pub fn field<S: ZclSchema>(&mut self, value: S::Value<'_>) -> Result<(), ZclError> {
        if self.field_count >= 0xFFFE {
            return Err(ZclError::InvalidLength);
        }
        let available = self
            .bytes
            .get_mut(self.offset..)
            .ok_or(ZclError::BufferTooSmall)?;
        if available.is_empty() {
            return Err(ZclError::BufferTooSmall);
        }
        available[0] = S::TYPE_ID.as_u8();
        let value_buf = available.get_mut(1..).ok_or(ZclError::BufferTooSmall)?;
        let used = S::encode(value, value_buf)?;
        self.offset += 1 + used;
        self.field_count += 1;
        Ok(())
    }

    /// Write `field_count` header and return total bytes written.
    pub fn finish(self) -> Result<usize, ZclError> {
        self.bytes[..2].copy_from_slice(&self.field_count.to_le_bytes());
        Ok(self.offset)
    }
}

/// Schema wrapper that lifts a `ZclStructSchema` into a full `ZclSchema`.
///
/// This enables `ArrayOf<StructOf<T>>`, `Nullable<StructOf<T>>`, and any other
/// composition that requires a `ZclSchema` type. Without this wrapper, `T`
/// implements `ZclStructSchema` but not `ZclSchema` directly — keeping the
/// two traits orthogonal and avoiding blanket coherence issues.
pub struct StructOf<T>(PhantomData<T>);

impl<T: ZclStructSchema> ZclSchema for StructOf<T> {
    type Value<'a>
        = T::Value<'a>
    where
        T: 'a;
    const TYPE_ID: TypeId = TypeId::Structure;
    const ENCODED_SIZE: Option<usize> = None;

    fn decode(bytes: &[u8]) -> Result<(T::Value<'_>, usize), ZclError> {
        let (value, used) = Self::decode_prefix(bytes)?;
        if used != bytes.len() {
            return Err(ZclError::InvalidLength);
        }
        Ok((value, used))
    }

    fn decode_prefix(bytes: &[u8]) -> Result<(T::Value<'_>, usize), ZclError> {
        let (mut decoder, _) = StructDecoder::new(bytes)?;
        let value = T::decode_fields(&mut decoder)?;
        let used = decoder.bytes_consumed();
        decoder.finish()?;
        Ok((value, used))
    }

    fn encode(value: T::Value<'_>, bytes: &mut [u8]) -> Result<usize, ZclError> {
        let mut encoder = StructEncoder::new(bytes)?;
        T::encode_fields(value, &mut encoder)?;
        encoder.finish()
    }
}

impl<T: ZclStructSchema> ZclHasNull for StructOf<T> {
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

/// Determine the byte length of `count` encoded elements in `payload`.
///
/// For fixed-size schemas, validates length arithmetic and (when
/// `!ALL_PATTERNS_VALID`) decodes each element to catch invalid bit patterns.
/// For variable-size schemas, decodes each element once to find boundaries.
fn compute_payload_len<S: ZclSchema>(payload: &[u8], count: u16) -> Result<usize, ZclError> {
    if let Some(elem_size) = S::ENCODED_SIZE {
        let total = (count as usize)
            .checked_mul(elem_size)
            .ok_or(ZclError::InvalidLength)?;
        if payload.len() < total {
            return Err(ZclError::InsufficientBytes);
        }
        if !S::ALL_PATTERNS_VALID {
            let mut offset = 0;
            for _ in 0..count {
                S::decode(&payload[offset..])?;
                offset += elem_size;
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
            let (_, used) = decode_schema_value::<S>(remaining)?;
            offset += used;
        }
        Ok(offset)
    }
}

fn decode_schema_value<S: ZclSchema>(bytes: &[u8]) -> Result<(S::Value<'_>, usize), ZclError> {
    let (value, used) = S::decode_prefix(bytes)?;
    if let Some(expected) = S::ENCODED_SIZE
        && used != expected
    {
        return Err(ZclError::InvalidLength);
    }
    Ok((value, used))
}

/// O(n^2) pairwise raw-slice uniqueness check. No allocation.
///
/// For fixed-size elements: compares fixed-length byte windows directly.
/// For variable-size elements: re-scans from start for each inner comparison.
/// Called only for `Set<RawUniqueSet>` after element structure is validated.
fn validate_set_uniqueness<S: ZclSchema>(payload: &[u8], count: u16) -> Result<(), ZclError> {
    if let Some(elem_size) = S::ENCODED_SIZE {
        for i in 0..count as usize {
            for j in 0..i {
                if payload[i * elem_size..(i + 1) * elem_size]
                    == payload[j * elem_size..(j + 1) * elem_size]
                {
                    return Err(ZclError::InvalidValue);
                }
            }
        }
    } else {
        let mut outer_offset = 0;
        for i in 0..count as usize {
            let (_, outer_len) = decode_schema_value::<S>(&payload[outer_offset..])?;
            let outer_elem = &payload[outer_offset..outer_offset + outer_len];

            let mut inner_offset = 0;
            for _ in 0..i {
                let (_, inner_len) = decode_schema_value::<S>(&payload[inner_offset..])?;
                let inner_elem = &payload[inner_offset..inner_offset + inner_len];
                if outer_elem == inner_elem {
                    return Err(ZclError::InvalidValue);
                }
                inner_offset += inner_len;
            }
            outer_offset += outer_len;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::nullable::Nullable;
    use crate::types::strings::ShortText;

    #[test]
    fn array_of_u16_roundtrip() {
        let wire: &[u8] = &[0x21, 0x03, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00];
        let (arr, used) = ArrayOf::<u16>::decode(wire).unwrap();
        assert_eq!(used, wire.len());
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn bare_array_rejects_null_sentinel() {
        let wire: &[u8] = &[0x21, 0xFF, 0xFF];
        assert_eq!(
            ArrayOf::<u16>::decode(wire).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn nullable_array_returns_none_for_null_sentinel() {
        let wire: &[u8] = &[0x21, 0xFF, 0xFF];
        let (v, n) = Nullable::<ArrayOf<u16>>::decode(wire).unwrap();
        assert_eq!(n, 3);
        assert!(v.is_none());
    }

    #[test]
    fn array_of_u16_type_mismatch() {
        let wire: &[u8] = &[0x20, 0x01, 0x00, 0x42];
        assert_eq!(
            ArrayOf::<u16>::decode(wire).unwrap_err(),
            ZclError::TypeIdMismatch {
                expected: TypeId::Uint16,
                found: TypeId::Uint8,
            }
        );
    }

    #[test]
    fn array_variable_size_insufficient_bytes() {
        let wire: &[u8] = &[0x42, 0x02, 0x00, 0x02, b'H', b'i'];
        assert_eq!(
            ArrayOf::<ShortText>::decode(wire).unwrap_err(),
            ZclError::InsufficientBytes
        );
    }

    #[test]
    fn struct_decoder_roundtrip() {
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x42, 0x21, 0x34, 0x12];
        let (mut dec, _) = StructDecoder::new(wire).unwrap();
        let v1: u8 = dec.field::<u8>().unwrap();
        let v2: u16 = dec.field::<u16>().unwrap();
        dec.finish().unwrap();
        assert_eq!(v1, 0x42);
        assert_eq!(v2, 0x1234);
    }

    #[test]
    fn struct_decoder_finish_rejects_unconsumed_fields() {
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x42, 0x21, 0x34, 0x12];
        let (mut dec, _) = StructDecoder::new(wire).unwrap();
        let _: u8 = dec.field::<u8>().unwrap();
        assert_eq!(dec.finish().unwrap_err(), ZclError::UnconsumedData);
    }

    #[test]
    fn struct_encoder_roundtrip() {
        let mut buf = [0u8; 16];
        let mut enc = StructEncoder::new(&mut buf).unwrap();
        enc.field::<u8>(0x42).unwrap();
        enc.field::<u16>(0x1234).unwrap();
        let n = enc.finish().unwrap();
        assert_eq!(n, 7);
        assert_eq!(&buf[..n], &[0x02, 0x00, 0x20, 0x42, 0x21, 0x34, 0x12]);
    }

    #[test]
    fn struct_encoder_rejects_reserved_field_count_before_write() {
        let mut buf = [0u8; 4];
        let mut enc = StructEncoder {
            bytes: &mut buf,
            offset: 2,
            field_count: 0xFFFE,
        };
        assert_eq!(enc.field::<u8>(0x42).unwrap_err(), ZclError::InvalidLength);
        assert_eq!(enc.field_count, 0xFFFE);
        assert_eq!(enc.offset, 2);
        assert_eq!(buf, [0, 0, 0, 0]);
    }

    #[test]
    fn struct_decoder_rejects_null_sentinel() {
        let wire: &[u8] = &[0xFF, 0xFF];
        assert_eq!(
            StructDecoder::new(wire).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn array_variable_size_rejects_trailing_bytes() {
        let wire: &[u8] = &[0x42, 0x01, 0x00, 0x01, b'A', 0x00];
        assert_eq!(
            ArrayOf::<ShortText>::decode(wire).unwrap_err(),
            ZclError::InvalidLength
        );
    }

    #[test]
    fn decode_attr_rejects_collection_trailing_bytes() {
        let wire: &[u8] = &[0x42, 0x01, 0x00, 0x01, b'A', 0x00];
        assert_eq!(
            crate::types::decode_attr::<ArrayOf<ShortText>>(TypeId::Array, wire).unwrap_err(),
            crate::types::AttrError::Codec(ZclError::InvalidLength)
        );
    }

    #[test]
    fn nested_variable_array_rejects_top_level_trailing_bytes() {
        let wire: &[u8] = &[
            0x48, 0x01, 0x00, // Array<Array<ShortText>>, count=1
            0x42, 0x01, 0x00, 0x01, b'A', // inner Array<ShortText>, count=1
            0x00, // trailing byte
        ];
        assert_eq!(
            ArrayOf::<ArrayOf<ShortText>>::decode(wire).unwrap_err(),
            ZclError::InvalidLength
        );
    }

    #[test]
    fn struct_decoder_handles_collection_before_following_field() {
        let wire: &[u8] = &[
            0x02, 0x00, // field_count
            0x48, // field 1 type: Array
            0x42, 0x01, 0x00, 0x01, b'A', // Array<ShortText>, count=1
            0x20, 0x7F, // field 2 type: Uint8
        ];
        let (mut dec, _) = StructDecoder::new(wire).unwrap();
        let arr = dec.field::<ArrayOf<ShortText>>().unwrap();
        let value: u8 = dec.field::<u8>().unwrap();
        dec.finish().unwrap();
        assert_eq!(arr.iter().next().unwrap().unwrap().as_str(), "A");
        assert_eq!(value, 0x7F);
    }

    #[test]
    fn nullable_set_and_bag_return_none_for_null_sentinel() {
        let wire: &[u8] = &[0x21, 0xFF, 0xFF];
        let (set, set_used) = Nullable::<SetOf<u16>>::decode(wire).unwrap();
        let (bag, bag_used) = Nullable::<BagOf<u16>>::decode(wire).unwrap();
        assert!(set.is_none());
        assert_eq!(set_used, 3);
        assert!(bag.is_none());
        assert_eq!(bag_used, 3);
    }

    #[test]
    fn dynamic_collection_iter_decodes_values() {
        let raw = ZclCollectionRef::new(
            CollectionKind::Array,
            RawTypeId::from_type_id(TypeId::Uint8),
            2,
            &[0x01, 0x02],
        );
        let mut iter = raw.iter();
        assert_eq!(iter.next().unwrap().unwrap(), ZclValueRef::Uint8(0x01));
        assert_eq!(iter.next().unwrap().unwrap(), ZclValueRef::Uint8(0x02));
        assert!(iter.next().is_none());
    }

    #[test]
    fn dynamic_struct_fields_iter_decodes_values() {
        let raw = ZclStructRef::new(2, &[0x20, 0x2A, 0x21, 0x34, 0x12]);
        let mut fields = raw.fields();
        let first = fields.next().unwrap().unwrap();
        assert_eq!(first.type_id, TypeId::Uint8);
        assert_eq!(first.value, ZclValueRef::Uint8(0x2A));
        let second = fields.next().unwrap().unwrap();
        assert_eq!(second.type_id, TypeId::Uint16);
        assert_eq!(second.value, ZclValueRef::Uint16(0x1234));
        assert!(fields.next().is_none());
    }

    struct TestStruct;

    impl ZclStructSchema for TestStruct {
        type Value<'a> = (u8, u16);

        fn decode_fields<'a>(decoder: &mut StructDecoder<'a>) -> Result<Self::Value<'a>, ZclError> {
            let first = decoder.field::<u8>()?;
            let second = decoder.field::<u16>()?;
            Ok((first, second))
        }

        fn encode_fields(
            value: Self::Value<'_>,
            encoder: &mut StructEncoder<'_>,
        ) -> Result<(), ZclError> {
            encoder.field::<u8>(value.0)?;
            encoder.field::<u16>(value.1)
        }
    }

    #[test]
    fn nullable_struct_of_returns_none_for_null_sentinel() {
        let (value, used) = Nullable::<StructOf<TestStruct>>::decode(&[0xFF, 0xFF]).unwrap();
        assert_eq!(value, None);
        assert_eq!(used, 2);
    }

    #[test]
    fn struct_of_roundtrips() {
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x2A, 0x21, 0x34, 0x12];
        let (decoded, used) = StructOf::<TestStruct>::decode(wire).unwrap();
        let mut buf = [0u8; 8];
        let encoded = StructOf::<TestStruct>::encode(decoded, &mut buf).unwrap();
        assert_eq!(decoded, (0x2A, 0x1234));
        assert_eq!(used, wire.len());
        assert_eq!(encoded, wire.len());
        assert_eq!(&buf[..encoded], wire);
    }
    #[test]
    fn struct_of_decode_rejects_trailing_bytes() {
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x2A, 0x21, 0x34, 0x12, 0x00];
        assert_eq!(
            StructOf::<TestStruct>::decode(wire).unwrap_err(),
            ZclError::InvalidLength
        );
    }

    #[test]
    fn decode_attr_rejects_struct_trailing_bytes() {
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x2A, 0x21, 0x34, 0x12, 0x00];
        assert_eq!(
            crate::types::decode_attr::<StructOf<TestStruct>>(TypeId::Structure, wire).unwrap_err(),
            crate::types::AttrError::Codec(ZclError::InvalidLength)
        );
    }

    #[test]
    fn array_of_struct_of_roundtrips() {
        // Array<Structure> with one element: field_count=2, u8=0x2A, u16=0x1234
        let wire: &[u8] = &[
            0x4C, 0x01, 0x00, // element_type=Structure, count=1
            0x02, 0x00, 0x20, 0x2A, 0x21, 0x34, 0x12, // StructOf<TestStruct>
        ];
        let (arr, used) = ArrayOf::<StructOf<TestStruct>>::decode(wire).unwrap();
        assert_eq!(used, wire.len());
        assert_eq!(arr.len(), 1);
        let (v1, v2) = arr.iter().next().unwrap().unwrap();
        assert_eq!(v1, 0x2A);
        assert_eq!(v2, 0x1234);
    }
    #[test]
    fn array_of_struct_of_uses_prefix_decode_for_multiple_elements() {
        let wire: &[u8] = &[
            0x4C, 0x02, 0x00, // element_type=Structure, count=2
            0x02, 0x00, 0x20, 0x2A, 0x21, 0x34, 0x12, // StructOf<TestStruct>
            0x02, 0x00, 0x20, 0x2B, 0x21, 0x35, 0x12, // StructOf<TestStruct>
        ];
        let (arr, used) = ArrayOf::<StructOf<TestStruct>>::decode(wire).unwrap();
        assert_eq!(used, wire.len());

        let mut iter = arr.iter();
        assert_eq!(iter.next().unwrap().unwrap(), (0x2A, 0x1234));
        assert_eq!(iter.next().unwrap().unwrap(), (0x2B, 0x1235));
        assert!(iter.next().is_none());
    }

    #[test]
    fn array_of_bool_rejects_invalid_element_at_decode() {
        let wire: &[u8] = &[0x10, 0x02, 0x00, 0x02, 0x01];
        assert_eq!(
            ArrayOf::<bool>::decode(wire).unwrap_err(),
            ZclError::InvalidValue
        );
    }

    #[test]
    fn array_of_bool_rejects_null_sentinel_at_decode() {
        let wire: &[u8] = &[0x10, 0x01, 0x00, 0xFF];
        assert_eq!(
            ArrayOf::<bool>::decode(wire).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn nullable_array_wrong_element_type_not_treated_as_null() {
        let wire: &[u8] = &[0x20, 0xFF, 0xFF]; // Uint8 type, not Uint16
        assert!(Nullable::<ArrayOf<u16>>::decode(wire).is_err());
    }

    #[test]
    fn struct_encoder_field_count_overflow_returns_error() {
        let mut buf = [0u8; 64];
        let mut enc = StructEncoder::new(&mut buf).unwrap();
        enc.field_count = u16::MAX;
        assert_eq!(enc.field::<u8>(0).unwrap_err(), ZclError::InvalidLength);
    }

    #[test]
    fn collection_encoder_u16_roundtrip() {
        let mut buf = [0u8; 16];
        let mut enc = CollectionEncoder::<Array, u16>::new(&mut buf).unwrap();
        enc.push(0x1234).unwrap();
        enc.push(0x5678).unwrap();
        let n = enc.finish().unwrap();
        assert_eq!(n, 7);
        assert_eq!(&buf[..n], &[0x21, 0x02, 0x00, 0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn collection_encoder_empty() {
        let mut buf = [0u8; 8];
        let enc = CollectionEncoder::<Array, u8>::new(&mut buf).unwrap();
        let n = enc.finish().unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[..n], &[0x20, 0x00, 0x00]);
    }

    #[test]
    fn collection_encoder_too_small_returns_error() {
        let mut buf = [0u8; 2];
        assert_eq!(
            CollectionEncoder::<Array, u8>::new(&mut buf).unwrap_err(),
            ZclError::BufferTooSmall
        );
    }

    #[test]
    fn collection_encoder_push_buffer_exhausted() {
        let mut buf = [0u8; 4]; // room for header + 1 byte
        let mut enc = CollectionEncoder::<Array, u8>::new(&mut buf).unwrap();
        enc.push(0x42).unwrap();
        assert_eq!(enc.push(0x43).unwrap_err(), ZclError::BufferTooSmall);
    }

    #[test]
    fn collection_encoder_count_overflow_returns_error() {
        let mut buf = [0u8; 8];
        let mut enc = CollectionEncoder::<Array, u8> {
            buf: &mut buf,
            offset: 3,
            count: 0xFFFE,
            _kind: PhantomData,
            _schema: PhantomData,
        };
        assert_eq!(enc.push(0x42).unwrap_err(), ZclError::InvalidLength);
    }

    #[test]
    fn collection_encoder_output_decodable() {
        let mut buf = [0u8; 16];
        let mut enc = CollectionEncoder::<Array, u16>::new(&mut buf).unwrap();
        enc.push(10u16).unwrap();
        enc.push(20u16).unwrap();
        enc.push(30u16).unwrap();
        let n = enc.finish().unwrap();

        let (arr, used) = ArrayOf::<u16>::decode(&buf[..n]).unwrap();
        assert_eq!(used, n);
        assert_eq!(arr.len(), 3);
        let mut iter = arr.iter();
        assert_eq!(iter.next().unwrap().unwrap(), 10u16);
        assert_eq!(iter.next().unwrap().unwrap(), 20u16);
        assert_eq!(iter.next().unwrap().unwrap(), 30u16);
        assert!(iter.next().is_none());
    }

    #[test]
    fn set_of_raw_unique_rejects_duplicate_fixed_elements() {
        // Set<Uint16>, count=3, elements=[0x0001, 0x0002, 0x0001] — duplicate
        let wire: &[u8] = &[0x21, 0x03, 0x00, 0x01, 0x00, 0x02, 0x00, 0x01, 0x00];
        assert_eq!(
            SetOf::<u16, RawUniqueSet>::decode(wire).unwrap_err(),
            ZclError::InvalidValue
        );
    }

    #[test]
    fn set_of_raw_unique_accepts_unique_elements() {
        let wire: &[u8] = &[0x21, 0x03, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00];
        let (s, used) = SetOf::<u16, RawUniqueSet>::decode(wire).unwrap();
        assert_eq!(used, wire.len());
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn set_of_unchecked_accepts_duplicate_elements() {
        // Default SetOf allows duplicates — no uniqueness check
        let wire: &[u8] = &[0x21, 0x03, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00];
        let (s, _) = SetOf::<u16>::decode(wire).unwrap();
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn array_of_i16_rejects_element_null_sentinel() {
        let wire: &[u8] = &[0x29, 0x01, 0x00, 0x00, 0x80];
        assert_eq!(
            ArrayOf::<i16>::decode(wire).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn array_of_nullable_i16_accepts_element_null_sentinel() {
        let wire: &[u8] = &[0x29, 0x01, 0x00, 0x00, 0x80];
        let (arr, used) = ArrayOf::<Nullable<i16>>::decode(wire).unwrap();
        let mut iter = arr.iter();
        assert_eq!(used, wire.len());
        assert_eq!(iter.next().unwrap().unwrap(), None);
        assert!(iter.next().is_none());
    }

    #[test]
    fn collection_decoder_streams_and_finishes() {
        let wire: &[u8] = &[0x42, 0x01, 0x00, 0x01, b'A'];
        let (arr, _) = ArrayOf::<ShortText>::decode(wire).unwrap();
        let mut decoder = arr.decoder();
        assert_eq!(decoder.next().unwrap().unwrap().as_str(), "A");
        assert!(decoder.next().is_none());
        decoder.finish().unwrap();
    }

    #[test]
    fn collection_decoder_finish_rejects_unconsumed_elements() {
        let wire: &[u8] = &[0x42, 0x02, 0x00, 0x01, b'A', 0x01, b'B'];
        let (arr, _) = ArrayOf::<ShortText>::decode(wire).unwrap();
        let mut decoder = arr.decoder();
        assert_eq!(decoder.next().unwrap().unwrap().as_str(), "A");
        // did not consume element "B"
        assert_eq!(decoder.finish().unwrap_err(), ZclError::UnconsumedData);
    }

    #[test]
    fn raw_unique_set_encoder_rejects_duplicate_elements() {
        let mut buf = [0u8; 16];
        let mut enc = CollectionEncoder::<Set<RawUniqueSet>, u16>::new(&mut buf).unwrap();
        enc.push(1).unwrap();
        enc.push(1).unwrap();
        assert_eq!(enc.finish().unwrap_err(), ZclError::InvalidValue);
    }

    enum TestSetPolicy {}

    impl SetPolicy for TestSetPolicy {}

    #[test]
    fn custom_set_policy_implements_collection_schema() {
        fn assert_schema<T: ZclSchema>() {}

        assert_schema::<SetOf<u8, TestSetPolicy>>();
    }
    #[test]
    fn collection_count_rejects_null_sentinel() {
        assert_eq!(
            CollectionCount::new(0xFFFF).unwrap_err(),
            ZclError::NullSentinel
        );
    }

    #[test]
    fn collection_count_accepts_zero_and_max() {
        assert_eq!(CollectionCount::new(0).unwrap().get(), 0);
        assert_eq!(CollectionCount::new(0xFFFE).unwrap().get(), 0xFFFE);
    }
}
