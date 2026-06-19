#![allow(dead_code)]

use core::cell::Cell;
use core::mem::size_of;

use crate::types::AccessFlags;
use crate::types::AttrError;
use crate::types::AttributeId;
use crate::types::TypeId;
use crate::types::ZclError;
use crate::types::descriptors::AttrInfo;

pub type ScalarStorage = u64;

/// Per-attribute descriptor stored in flash/ROM. A `ClusterServer` holds a
/// `&'static [AttrDescriptor]` sorted ascending by `attr`. Binary search
/// requires strict ascending order — use `is_sorted` and
/// `has_no_duplicate_keys` const assertions at the definition site.
#[derive(Clone, Copy)]
pub struct AttrDescriptor {
    pub attr: AttributeId,
    pub access: AccessFlags,
    pub type_id: TypeId,
    pub storage: StorageKind,
}

/// How a scalar or string attribute is backed.
#[allow(dead_code)]
// Const/static variants are exercised by unit tests; only DeviceEnabled uses this store in current
// standard clusters.
#[derive(Clone, Copy)]
pub enum StorageKind {
    /// Immutable scalar encoded from a compile-time constant.
    ConstScalar(ScalarStorage),
    /// Mutable scalar backed by a `Cell<u64>` slot in `SplitAttributeStore`.
    MutableScalar { index: u8 },
    /// Immutable string backed by a `'static` reference.
    StaticString(StaticStringValue),
}

/// Static string payload for `StorageKind::StaticString`.
#[allow(dead_code)] // Constructed by store tests and future store-backed clusters.
#[derive(Clone, Copy)]
pub enum StaticStringValue {
    /// ZCL `OctetString` (`type_id` 0x41) — short, 1-byte length prefix.
    Octets(&'static [u8]),
    /// ZCL `CharacterString` (`type_id` 0x42) — short, 1-byte length prefix.
    Text(&'static str),
}

/// Flash/RAM-split attribute storage. `descriptors` lives in ROM; `mutable`
/// lives in RAM. `Cell<u64>` provides interior mutability without `&mut self`,
/// keeping `ClusterServer::read_attribute` callable via `&self`.
///
/// Not a public trait and not part of the stable API surface. Used internally
/// by `ClusterServer` Strategy 2 implementations (e.g. `BasicServer`).
pub(crate) struct SplitAttributeStore<const MUT_N: usize> {
    descriptors: &'static [AttrDescriptor],
    mutable: [Cell<u64>; MUT_N],
}

impl<const MUT_N: usize> SplitAttributeStore<MUT_N> {
    pub const fn new(descriptors: &'static [AttrDescriptor], mutable: [Cell<u64>; MUT_N]) -> Self {
        Self {
            descriptors,
            mutable,
        }
    }

    fn find_entry(&self, attr: AttributeId) -> Option<&AttrDescriptor> {
        let target = attr.0;
        let idx = self.descriptors.partition_point(|e| e.attr.0 < target);
        self.descriptors.get(idx).filter(|e| e.attr == attr)
    }

    /// Encode attribute `id` into `buf`. Returns `(TypeId, bytes_written)`.
    pub fn read_into(&self, id: AttributeId, buf: &mut [u8]) -> Result<(TypeId, usize), AttrError> {
        let desc = self.find_entry(id).ok_or(AttrError::UnsupportedAttribute)?;
        if !desc.access.is_readable() {
            return Err(AttrError::WriteOnly);
        }
        match &desc.storage {
            StorageKind::ConstScalar(val) => encode_scalar(desc.type_id, *val, buf),
            StorageKind::MutableScalar { index } => {
                let val = self
                    .mutable
                    .get(usize::from(*index))
                    .ok_or(AttrError::InvalidValue)?
                    .get();
                encode_scalar(desc.type_id, val, buf)
            }
            StorageKind::StaticString(sv) => encode_static_string(desc.type_id, *sv, buf),
        }
    }

    /// Validate a write without mutating state. Checks access flags, `type_id`,
    /// payload length, and storage mutability.
    pub fn check_write_from(
        &self,
        id: AttributeId,
        type_id: TypeId,
        data: &[u8],
    ) -> Result<(), AttrError> {
        let desc = self.find_entry(id).ok_or(AttrError::UnsupportedAttribute)?;
        check_write(desc, type_id, data)
    }

    /// Visit descriptor-backed attribute metadata in descriptor order.
    pub fn visit_attributes(
        &self,
        visitor: &mut dyn FnMut(AttrInfo) -> Result<(), ZclError>,
    ) -> Result<(), ZclError> {
        for desc in self.descriptors {
            visitor(AttrInfo {
                id: desc.attr,
                type_id: desc.type_id,
                access: desc.access,
            })?;
        }
        Ok(())
    }

    /// Decode `data` and store into the mutable slot for `id`. Takes `&self`
    /// because `Cell<u64>` provides interior mutability.
    pub fn write_from(
        &self,
        id: AttributeId,
        type_id: TypeId,
        data: &[u8],
    ) -> Result<(), AttrError> {
        let desc = self.find_entry(id).ok_or(AttrError::UnsupportedAttribute)?;
        check_write(desc, type_id, data)?;
        match &desc.storage {
            StorageKind::MutableScalar { index } => {
                let size = desc
                    .type_id
                    .fixed_size()
                    .ok_or(AttrError::Codec(ZclError::InvalidLength))?;
                let mut raw = [0u8; 8];
                raw[..size].copy_from_slice(&data[..size]);
                self.mutable
                    .get(usize::from(*index))
                    .ok_or(AttrError::InvalidValue)?
                    .set(u64::from_le_bytes(raw));
                Ok(())
            }
            StorageKind::ConstScalar(_) | StorageKind::StaticString(_) => Err(AttrError::ReadOnly),
        }
    }
}

fn check_write(desc: &AttrDescriptor, type_id: TypeId, data: &[u8]) -> Result<(), AttrError> {
    if !desc.access.is_writable() {
        return Err(AttrError::ReadOnly);
    }
    if type_id != desc.type_id {
        return Err(AttrError::InvalidDataType);
    }
    match &desc.storage {
        StorageKind::MutableScalar { .. } => validate_scalar_write(desc.type_id, data),
        StorageKind::ConstScalar(_) | StorageKind::StaticString(_) => Err(AttrError::ReadOnly),
    }
}

fn validate_scalar_write(type_id: TypeId, data: &[u8]) -> Result<(), AttrError> {
    let size = type_id
        .fixed_size()
        .ok_or(AttrError::Codec(ZclError::InvalidLength))?;
    if size > core::mem::size_of::<ScalarStorage>() {
        return Err(AttrError::InvalidDataType);
    }
    if data.len() < size {
        return Err(AttrError::Codec(ZclError::InsufficientBytes));
    }
    if data.len() != size {
        return Err(AttrError::Codec(ZclError::InvalidLength));
    }
    if type_id == TypeId::Boolean && !matches!(data[0], 0x00 | 0x01) {
        return Err(AttrError::Codec(ZclError::InvalidValue));
    }
    Ok(())
}

fn encode_scalar(type_id: TypeId, val: u64, buf: &mut [u8]) -> Result<(TypeId, usize), AttrError> {
    let size = type_id
        .fixed_size()
        .ok_or(AttrError::Codec(ZclError::InvalidValue))?;
    if size > size_of::<ScalarStorage>() {
        return Err(AttrError::InvalidDataType);
    }
    if buf.len() < size {
        return Err(AttrError::Codec(ZclError::BufferTooSmall));
    }
    buf[..size].copy_from_slice(&val.to_le_bytes()[..size]);
    Ok((type_id, size))
}

#[allow(clippy::cast_possible_truncation)]
fn encode_static_string(
    type_id: TypeId,
    sv: StaticStringValue,
    buf: &mut [u8],
) -> Result<(TypeId, usize), AttrError> {
    let raw: &[u8] = match sv {
        StaticStringValue::Text(s) => s.as_bytes(),
        StaticStringValue::Octets(b) => b,
    };
    let len = raw.len();
    if len > 254 {
        return Err(AttrError::Codec(ZclError::InvalidLength));
    }
    let total = 1 + len;
    if buf.len() < total {
        return Err(AttrError::Codec(ZclError::BufferTooSmall));
    }
    buf[0] = len as u8; // len ≤ 254
    buf[1..total].copy_from_slice(raw);
    Ok((type_id, total))
}

/// Returns `true` when `entries` is strictly ascending by `attr.0`. Required
/// for the binary search in `find_entry` to be correct.
pub const fn is_sorted(entries: &[AttrDescriptor]) -> bool {
    let mut i = 1;
    while i < entries.len() {
        if entries[i].attr.0 <= entries[i - 1].attr.0 {
            return false;
        }
        i += 1;
    }
    true
}

/// Returns `true` when no two entries share the same `attr.0`.
pub const fn has_no_duplicate_keys(entries: &[AttrDescriptor]) -> bool {
    let mut i = 1;
    while i < entries.len() {
        if entries[i].attr.0 == entries[i - 1].attr.0 {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── test descriptor tables ────────────────────────────────────────────────

    static ATTRS: &[AttrDescriptor] = &[
        AttrDescriptor {
            attr: AttributeId::new(0x0000),
            access: AccessFlags::READ,
            type_id: TypeId::Uint8,
            storage: StorageKind::ConstScalar(4),
        },
        AttrDescriptor {
            attr: AttributeId::new(0x0001),
            access: AccessFlags::READ_WRITE,
            type_id: TypeId::Uint16,
            storage: StorageKind::MutableScalar { index: 0 },
        },
        AttrDescriptor {
            attr: AttributeId::new(0x0002),
            access: AccessFlags::READ,
            type_id: TypeId::CharacterString,
            storage: StorageKind::StaticString(StaticStringValue::Text("Acme")),
        },
        AttrDescriptor {
            attr: AttributeId::new(0x0003),
            access: AccessFlags::READ,
            type_id: TypeId::OctetString,
            storage: StorageKind::StaticString(StaticStringValue::Octets(b"\x01\x02\x03")),
        },
        AttrDescriptor {
            attr: AttributeId::new(0x0004),
            access: AccessFlags::READ_WRITE,
            type_id: TypeId::Boolean,
            storage: StorageKind::MutableScalar { index: 1 },
        },
    ];

    const _: () = assert!(is_sorted(ATTRS));
    const _: () = assert!(has_no_duplicate_keys(ATTRS));

    fn make_store() -> SplitAttributeStore<2> {
        SplitAttributeStore::new(ATTRS, [Cell::new(0x1234), Cell::new(0x00)])
    }

    // ── read_into ─────────────────────────────────────────────────────────────

    #[test]
    fn read_const_scalar() {
        let store = make_store();
        let mut buf = [0u8; 4];
        let (tid, n) = store.read_into(AttributeId::new(0x0000), &mut buf).unwrap();
        assert_eq!(tid, TypeId::Uint8);
        assert_eq!(n, 1);
        assert_eq!(buf[0], 4);
    }

    #[test]
    fn read_mutable_scalar_initial_value() {
        let store = make_store();
        let mut buf = [0u8; 4];
        let (tid, n) = store.read_into(AttributeId::new(0x0001), &mut buf).unwrap();
        assert_eq!(tid, TypeId::Uint16);
        assert_eq!(n, 2);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0x1234);
    }

    #[test]
    fn read_static_text_string() {
        let store = make_store();
        let mut buf = [0u8; 16];
        let (tid, n) = store.read_into(AttributeId::new(0x0002), &mut buf).unwrap();
        assert_eq!(tid, TypeId::CharacterString);
        // 1-byte length + "Acme"
        assert_eq!(n, 5);
        assert_eq!(buf[0], 4); // len = 4
        assert_eq!(&buf[1..5], b"Acme");
    }

    #[test]
    fn read_static_octet_string() {
        let store = make_store();
        let mut buf = [0u8; 16];
        let (tid, n) = store.read_into(AttributeId::new(0x0003), &mut buf).unwrap();
        assert_eq!(tid, TypeId::OctetString);
        assert_eq!(n, 4); // 1 + 3
        assert_eq!(buf[0], 3);
        assert_eq!(&buf[1..4], b"\x01\x02\x03");
    }

    #[test]
    fn read_unknown_attr_returns_unsupported() {
        let store = make_store();
        let mut buf = [0u8; 4];
        assert_eq!(
            store.read_into(AttributeId::new(0xFFFF), &mut buf),
            Err(AttrError::UnsupportedAttribute)
        );
    }

    #[test]
    fn read_buf_too_small_returns_codec_error() {
        let store = make_store();
        let mut buf = [0u8; 1];
        assert_eq!(
            store.read_into(AttributeId::new(0x0001), &mut buf), // Uint16 needs 2 bytes
            Err(AttrError::Codec(ZclError::BufferTooSmall))
        );
    }

    // ── write_from ────────────────────────────────────────────────────────────

    #[test]
    fn write_mutable_scalar_visible_on_next_read() {
        let store = make_store();
        let data = 0xABCDu16.to_le_bytes();
        store
            .write_from(AttributeId::new(0x0001), TypeId::Uint16, &data)
            .unwrap();
        let mut buf = [0u8; 4];
        let (_, n) = store.read_into(AttributeId::new(0x0001), &mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0xABCD);
    }

    #[test]
    fn write_to_read_only_attr_returns_read_only() {
        let store = make_store();
        let data = [5u8];
        let before = {
            let mut buf = [0u8; 4];
            store.read_into(AttributeId::new(0x0000), &mut buf).unwrap();
            buf
        };
        assert_eq!(
            store.write_from(AttributeId::new(0x0000), TypeId::Uint8, &data),
            Err(AttrError::ReadOnly)
        );
        // value unchanged
        let mut buf = [0u8; 4];
        store.read_into(AttributeId::new(0x0000), &mut buf).unwrap();
        assert_eq!(buf, before);
    }

    #[test]
    fn write_type_mismatch_returns_invalid_data_type() {
        let store = make_store();
        let data = [0u8; 4];
        assert_eq!(
            store.write_from(AttributeId::new(0x0001), TypeId::Uint32, &data),
            Err(AttrError::InvalidDataType)
        );
    }

    #[test]
    fn write_unknown_attr_returns_unsupported() {
        let store = make_store();
        assert_eq!(
            store.write_from(AttributeId::new(0xDEAD), TypeId::Uint8, &[0]),
            Err(AttrError::UnsupportedAttribute)
        );
    }

    #[test]
    fn write_static_string_attr_returns_read_only() {
        let store = make_store();
        // Even if somehow marked writable, StaticString is not mutable
        assert_eq!(
            store.write_from(
                AttributeId::new(0x0002),
                TypeId::CharacterString,
                &[0x02, b'H', b'i']
            ),
            Err(AttrError::ReadOnly)
        );
    }

    // ── check_write_from ──────────────────────────────────────────────────────

    #[test]
    fn check_write_passes_for_valid_mutable_write() {
        let store = make_store();
        let data = 100u16.to_le_bytes();
        assert!(
            store
                .check_write_from(AttributeId::new(0x0001), TypeId::Uint16, &data)
                .is_ok()
        );
    }

    #[test]
    fn check_write_read_only_attr() {
        let store = make_store();
        assert_eq!(
            store.check_write_from(AttributeId::new(0x0000), TypeId::Uint8, &[5]),
            Err(AttrError::ReadOnly)
        );
    }

    #[test]
    fn check_write_type_mismatch() {
        let store = make_store();
        assert_eq!(
            store.check_write_from(AttributeId::new(0x0001), TypeId::Uint8, &[5]),
            Err(AttrError::InvalidDataType)
        );
    }

    #[test]
    fn check_write_insufficient_data() {
        let store = make_store();
        // Uint16 needs 2 bytes; supply 1
        assert_eq!(
            store.check_write_from(AttributeId::new(0x0001), TypeId::Uint16, &[0x01]),
            Err(AttrError::Codec(ZclError::InsufficientBytes))
        );
    }

    // ── const validators ──────────────────────────────────────────────────────

    #[test]
    fn is_sorted_rejects_out_of_order() {
        static BAD: &[AttrDescriptor] = &[
            AttrDescriptor {
                attr: AttributeId::new(0x0002),
                access: AccessFlags::READ,
                type_id: TypeId::Uint8,
                storage: StorageKind::ConstScalar(0),
            },
            AttrDescriptor {
                attr: AttributeId::new(0x0001),
                access: AccessFlags::READ,
                type_id: TypeId::Uint8,
                storage: StorageKind::ConstScalar(0),
            },
        ];
        assert!(!is_sorted(BAD));
    }

    #[test]
    fn has_no_duplicate_keys_rejects_duplicates() {
        static DUP: &[AttrDescriptor] = &[
            AttrDescriptor {
                attr: AttributeId::new(0x0001),
                access: AccessFlags::READ,
                type_id: TypeId::Uint8,
                storage: StorageKind::ConstScalar(0),
            },
            AttrDescriptor {
                attr: AttributeId::new(0x0001),
                access: AccessFlags::READ,
                type_id: TypeId::Uint8,
                storage: StorageKind::ConstScalar(0),
            },
        ];
        assert!(!has_no_duplicate_keys(DUP));
    }

    #[test]
    fn boolean_mutable_write_and_read() {
        let store = make_store();
        // Write true (0x01)
        store
            .write_from(AttributeId::new(0x0004), TypeId::Boolean, &[0x01])
            .unwrap();
        let mut buf = [0u8; 4];
        let (tid, n) = store.read_into(AttributeId::new(0x0004), &mut buf).unwrap();
        assert_eq!(tid, TypeId::Boolean);
        assert_eq!(n, 1);
        assert_eq!(buf[0], 0x01);
    }

    #[test]
    fn read_write_only_attr_returns_write_only() {
        static WRITE_ONLY: &[AttrDescriptor] = &[AttrDescriptor {
            attr: AttributeId::new(0x0100),
            access: AccessFlags::WRITE,
            type_id: TypeId::Uint8,
            storage: StorageKind::MutableScalar { index: 0 },
        }];
        let store = SplitAttributeStore::new(WRITE_ONLY, [Cell::new(7u64)]);
        let mut buf = [0u8; 1];
        assert_eq!(
            store.read_into(AttributeId::new(0x0100), &mut buf),
            Err(AttrError::WriteOnly)
        );
    }

    #[test]
    fn bad_mutable_index_returns_attribute_error() {
        static BAD_INDEX: &[AttrDescriptor] = &[AttrDescriptor {
            attr: AttributeId::new(0x0101),
            access: AccessFlags::READ_WRITE,
            type_id: TypeId::Uint8,
            storage: StorageKind::MutableScalar { index: 1 },
        }];
        let store = SplitAttributeStore::new(BAD_INDEX, [Cell::new(0u64)]);
        let mut buf = [0u8; 1];
        assert_eq!(
            store.read_into(AttributeId::new(0x0101), &mut buf),
            Err(AttrError::InvalidValue)
        );
        assert_eq!(
            store.write_from(AttributeId::new(0x0101), TypeId::Uint8, &[1]),
            Err(AttrError::InvalidValue)
        );
    }

    #[test]
    fn scalar_storage_rejects_wide_fixed_types() {
        static WIDE: &[AttrDescriptor] = &[AttrDescriptor {
            attr: AttributeId::new(0x0102),
            access: AccessFlags::READ_WRITE,
            type_id: TypeId::SecurityKey,
            storage: StorageKind::ConstScalar(0),
        }];
        let store = SplitAttributeStore::<0>::new(WIDE, []);
        let mut buf = [0u8; 16];
        assert_eq!(
            store.read_into(AttributeId::new(0x0102), &mut buf),
            Err(AttrError::InvalidDataType)
        );
    }
}
