//! Ordered and unordered collection data types.
//!
//! See ZCL Section 2.6.2.14 to 2.6.2.17.
//!
//! An array, set or bag is a two-byte element count followed by that many
//! values of one element type, all sharing the type identifier written once by
//! the collection itself. A structure instead carries a count of
//! `type | value` pairs, each free to differ.
//!
//! Collections borrow the frame: reading one validates the elements and yields
//! a reference that iterates them on demand, so no element storage is needed.

use core::marker::PhantomData;

use byte::BytesExt;
use byte::ctx;

use super::codec::ZclKind;
use super::ids::TypeId;

/// Count of elements or fields, capped one below the all-ones non-value
/// (2.6.2.14).
pub const MAX_COUNT: u16 = 0xFFFE;

/// The shape of an element-homogeneous collection.
pub trait CollectionKindTypestate {
    /// Type identifier the collection is written with.
    const TYPE_ID: TypeId;
}

/// An ordered collection (2.6.2.14).
pub enum Array {}
/// An unordered collection without duplicates (2.6.2.15).
pub enum Set {}
/// An unordered collection permitting duplicates (2.6.2.16).
pub enum Bag {}

impl CollectionKindTypestate for Array {
    const TYPE_ID: TypeId = TypeId::Array;
}
impl CollectionKindTypestate for Set {
    const TYPE_ID: TypeId = TypeId::Set;
}
impl CollectionKindTypestate for Bag {
    const TYPE_ID: TypeId = TypeId::Bag;
}

/// A validated collection borrowed from the frame.
///
/// Yields elements by iteration; every element was decoded once during
/// validation, so iteration cannot fail.
pub struct CollectionRef<'a, K, S> {
    body: &'a [u8],
    count: u16,
    _kind: PhantomData<(K, S)>,
}

impl<K, S> Clone for CollectionRef<'_, K, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<K, S> Copy for CollectionRef<'_, K, S> {}

impl<K, S> core::fmt::Debug for CollectionRef<'_, K, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CollectionRef")
            .field("count", &self.count)
            .finish()
    }
}

impl<'a, K, S: ZclKind> CollectionRef<'a, K, S> {
    /// Number of elements.
    pub const fn len(&self) -> u16 {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Elements, in wire order.
    pub const fn iter(&self) -> CollectionIter<'a, S> {
        CollectionIter {
            body: self.body,
            offset: 0,
            left: self.count,
            _schema: PhantomData,
        }
    }
}

impl<'a, K, S: ZclKind> IntoIterator for &CollectionRef<'a, K, S> {
    type Item = S::Value<'a>;
    type IntoIter = CollectionIter<'a, S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over a validated collection's elements.
pub struct CollectionIter<'a, S> {
    body: &'a [u8],
    offset: usize,
    left: u16,
    _schema: PhantomData<S>,
}

impl<'a, S: ZclKind> Iterator for CollectionIter<'a, S> {
    type Item = S::Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        // validated on read, so an element cannot fail to decode here
        S::read(self.body, &mut self.offset).ok()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::from(self.left), Some(usize::from(self.left)))
    }
}

/// An element-homogeneous collection of `S` (2.6.2.14 to 2.6.2.16).
pub struct CollectionOf<K, S>(PhantomData<(K, S)>);

/// `array` of `S` (2.6.2.14).
pub type ArrayOf<S> = CollectionOf<Array, S>;
/// `set` of `S` (2.6.2.15).
pub type SetOf<S> = CollectionOf<Set, S>;
/// `bag` of `S` (2.6.2.16).
pub type BagOf<S> = CollectionOf<Bag, S>;

impl<K: CollectionKindTypestate, S: ZclKind> ZclKind for CollectionOf<K, S> {
    type Value<'a> = CollectionRef<'a, K, S>;
    const TYPE_ID: TypeId = K::TYPE_ID;
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<CollectionRef<'a, K, S>> {
        let count: u16 = bytes.read_with(offset, ctx::LE)?;
        if count > MAX_COUNT {
            return Err(bad_input!("collection non-value"));
        }

        // the element type is written once, ahead of the elements
        let element_type = TypeId::from_u8(bytes.read_with(offset, ctx::LE)?);
        if element_type != S::TYPE_ID {
            return Err(bad_input!("collection element type mismatch"));
        }

        let start = *offset;
        match S::ENCODED_SIZE {
            // a fixed-width element type where every pattern decodes needs no
            // per-element work: the length alone proves the body is well formed
            Some(width) if S::ALL_PATTERNS_VALID => {
                let span = width * usize::from(count);
                if bytes.len() < start + span {
                    return Err(byte::Error::Incomplete);
                }
                *offset += span;
            }
            _ => {
                for _ in 0..count {
                    S::read(bytes, offset)?;
                }
            }
        }

        Ok(CollectionRef {
            body: bytes.get(start..*offset).ok_or(byte::Error::Incomplete)?,
            count,
            _kind: PhantomData,
        })
    }

    fn write(
        value: CollectionRef<'_, K, S>,
        bytes: &mut [u8],
        offset: &mut usize,
    ) -> byte::Result<()> {
        bytes.write_with(offset, value.count, ctx::LE)?;
        bytes.write_with(offset, S::TYPE_ID.as_u8(), ctx::LE)?;
        bytes
            .get_mut(*offset..*offset + value.body.len())
            .ok_or(byte::Error::Incomplete)?
            .copy_from_slice(value.body);
        *offset += value.body.len();
        Ok(())
    }
}

/// Builds an element-homogeneous collection directly into a buffer.
///
/// The element count is only known once the elements are written, so it is
/// reserved on [`new`](Self::new) and patched by [`finish`](Self::finish).
pub struct CollectionEncoder<'a, K, S> {
    bytes: &'a mut [u8],
    count_at: usize,
    offset: usize,
    count: u16,
    _kind: PhantomData<(K, S)>,
}

impl<'a, K: CollectionKindTypestate, S: ZclKind> CollectionEncoder<'a, K, S> {
    /// Start a collection at `offset`, writing the count placeholder and the
    /// element type.
    pub fn new(bytes: &'a mut [u8], offset: usize) -> byte::Result<Self> {
        let count_at = offset;
        let offset = &mut { offset };
        bytes.write_with(offset, 0u16, ctx::LE)?;
        bytes.write_with(offset, S::TYPE_ID.as_u8(), ctx::LE)?;

        Ok(Self {
            bytes,
            count_at,
            offset: *offset,
            count: 0,
            _kind: PhantomData,
        })
    }

    /// Append one element.
    pub fn push(&mut self, value: S::Value<'_>) -> byte::Result<()> {
        if self.count == MAX_COUNT {
            return Err(bad_input!("collection is full"));
        }
        S::write(value, self.bytes, &mut self.offset)?;
        self.count += 1;
        Ok(())
    }

    /// Patch the count in and report the offset past the collection.
    pub fn finish(self) -> byte::Result<usize> {
        let patch = &mut { self.count_at };
        self.bytes.write_with(patch, self.count, ctx::LE)?;
        Ok(self.offset)
    }
}

/// The fields of one structure, in order (2.6.2.17).
///
/// Implemented by the type describing a structure attribute; the decoder walks
/// the wire pairs and hands each to [`ZclStruct::field`].
pub trait ZclStruct {
    /// Number of fields the structure carries.
    const FIELDS: u16;

    /// Validate field `index`, whose wire type is `type_id`, reading its value
    /// from `bytes` at `offset`.
    fn field(index: u16, type_id: TypeId, bytes: &[u8], offset: &mut usize) -> byte::Result<()>;
}

/// A validated structure borrowed from the frame.
pub struct StructRef<'a, T> {
    body: &'a [u8],
    count: u16,
    _fields: PhantomData<T>,
}

impl<T> Clone for StructRef<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for StructRef<'_, T> {}

impl<T> core::fmt::Debug for StructRef<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StructRef")
            .field("count", &self.count)
            .finish()
    }
}

impl<'a, T> StructRef<'a, T> {
    /// Number of fields.
    pub const fn len(&self) -> u16 {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The field bytes, as `type | value` pairs.
    pub const fn body(&self) -> &'a [u8] {
        self.body
    }
}

/// A `structure` whose fields are described by `T` (2.6.2.17).
pub struct StructOf<T>(PhantomData<T>);

impl<T: ZclStruct> ZclKind for StructOf<T> {
    type Value<'a> = StructRef<'a, T>;
    const TYPE_ID: TypeId = TypeId::Structure;
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<StructRef<'a, T>> {
        let count: u16 = bytes.read_with(offset, ctx::LE)?;
        if count != T::FIELDS {
            return Err(bad_input!("structure field count mismatch"));
        }

        let start = *offset;
        for index in 0..count {
            // each field carries its own type, unlike a collection's elements
            let type_id = TypeId::from_u8(bytes.read_with(offset, ctx::LE)?);
            T::field(index, type_id, bytes, offset)?;
        }

        Ok(StructRef {
            body: bytes.get(start..*offset).ok_or(byte::Error::Incomplete)?,
            count,
            _fields: PhantomData,
        })
    }

    fn write(value: StructRef<'_, T>, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
        bytes.write_with(offset, value.count, ctx::LE)?;
        bytes
            .get_mut(*offset..*offset + value.body.len())
            .ok_or(byte::Error::Incomplete)?
            .copy_from_slice(value.body);
        *offset += value.body.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::integers::Uint8;
    use crate::types::integers::Uint16;
    use crate::types::strings::ShortStr;
    use crate::types::strings::ShortText;

    // 2.6.2.14: count, element type, then the elements
    #[test]
    fn an_array_of_fixed_width_elements_round_trips() {
        let wire: &[u8] = &[0x03, 0x00, 0x20, 0x0a, 0x14, 0x1e];

        let offset = &mut 0;
        let array = ArrayOf::<Uint8>::read(wire, offset).expect("array decoded");
        assert_eq!(array.len(), 3);
        assert_eq!(*offset, wire.len());

        let values: heapless::Vec<Uint8, 4> = array.iter().collect();
        assert_eq!(values.as_slice(), &[Uint8(10), Uint8(20), Uint8(30)]);
    }

    #[test]
    fn a_variable_width_element_type_is_walked() {
        let wire: &[u8] = &[0x02, 0x00, 0x42, 0x02, b'h', b'i', 0x03, b'y', b'e', b's'];

        let array = ArrayOf::<ShortText>::read(wire, &mut 0).expect("array decoded");
        let values: heapless::Vec<ShortStr<'_>, 4> = array.iter().collect();
        assert_eq!(values[0].as_str(), "hi");
        assert_eq!(values[1].as_str(), "yes");
    }

    #[test]
    fn an_element_type_that_disagrees_is_rejected() {
        // declares uint16 elements, decoded as an array of uint8
        let wire: &[u8] = &[0x01, 0x00, 0x21, 0x0a, 0x00];
        assert!(ArrayOf::<Uint8>::read(wire, &mut 0).is_err());
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        let wire: &[u8] = &[0x03, 0x00, 0x20, 0x0a];
        assert!(ArrayOf::<Uint8>::read(wire, &mut 0).is_err());
    }

    // 2.6.2.14: 0xFFFF elements is the collection non-value
    #[test]
    fn the_collection_non_value_is_rejected() {
        let wire: &[u8] = &[0xFF, 0xFF, 0x20];
        assert!(ArrayOf::<Uint8>::read(wire, &mut 0).is_err());
    }

    #[test]
    fn a_set_and_a_bag_differ_only_in_type_identifier() {
        assert_eq!(<SetOf<Uint8> as ZclKind>::TYPE_ID, TypeId::Set);
        assert_eq!(<BagOf<Uint8> as ZclKind>::TYPE_ID, TypeId::Bag);
        assert_eq!(<ArrayOf<Uint8> as ZclKind>::TYPE_ID, TypeId::Array);
    }

    #[test]
    fn the_encoder_patches_the_count_in() {
        let mut buf = [0u8; 16];
        let mut encoder = CollectionEncoder::<Array, Uint8>::new(&mut buf, 0).expect("started");
        encoder.push(Uint8(10)).expect("pushed");
        encoder.push(Uint8(20)).expect("pushed");
        let end = encoder.finish().expect("finished");

        assert_eq!(&buf[..end], &[0x02, 0x00, 0x20, 0x0a, 0x14]);
        let array = ArrayOf::<Uint8>::read(&buf[..end], &mut 0).expect("decoded");
        assert_eq!(array.len(), 2);
    }

    struct Reading;

    impl ZclStruct for Reading {
        const FIELDS: u16 = 2;

        fn field(
            index: u16,
            type_id: TypeId,
            bytes: &[u8],
            offset: &mut usize,
        ) -> byte::Result<()> {
            match index {
                0 => Uint8::read_typed(type_id, bytes, offset).map(|_| ()),
                _ => Uint16::read_typed(type_id, bytes, offset).map(|_| ()),
            }
        }
    }

    // 2.6.2.17: count, then a type/value pair per field
    #[test]
    fn a_structure_validates_each_field_against_its_own_type() {
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x2a, 0x21, 0x34, 0x12];

        let offset = &mut 0;
        let value = StructOf::<Reading>::read(wire, offset).expect("structure decoded");
        assert_eq!(value.len(), 2);
        assert_eq!(*offset, wire.len());
    }

    #[test]
    fn a_structure_field_of_the_wrong_type_is_rejected() {
        // second field declared uint8 where the schema wants uint16
        let wire: &[u8] = &[0x02, 0x00, 0x20, 0x2a, 0x20, 0x34];
        assert!(StructOf::<Reading>::read(wire, &mut 0).is_err());
    }

    #[test]
    fn a_structure_with_the_wrong_field_count_is_rejected() {
        let wire: &[u8] = &[0x03, 0x00, 0x20, 0x2a, 0x21, 0x34, 0x12];
        assert!(StructOf::<Reading>::read(wire, &mut 0).is_err());
    }
}
