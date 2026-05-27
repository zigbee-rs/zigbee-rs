pub use crate::header::manufacturer_code::ManufacturerCode;

/// ZCL wire data type identifier (ZCL spec rev8 §2.6.2).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeId {
    NoData = 0x00,
    Data8 = 0x08,
    Data16 = 0x09,
    Data24 = 0x0A,
    Data32 = 0x0B,
    Data40 = 0x0C,
    Data48 = 0x0D,
    Data56 = 0x0E,
    Data64 = 0x0F,
    Boolean = 0x10,
    Bitmap8 = 0x18,
    Bitmap16 = 0x19,
    Bitmap24 = 0x1A,
    Bitmap32 = 0x1B,
    Bitmap40 = 0x1C,
    Bitmap48 = 0x1D,
    Bitmap56 = 0x1E,
    Bitmap64 = 0x1F,
    Uint8 = 0x20,
    Uint16 = 0x21,
    Uint24 = 0x22,
    Uint32 = 0x23,
    Uint40 = 0x24,
    Uint48 = 0x25,
    Uint56 = 0x26,
    Uint64 = 0x27,
    Int8 = 0x28,
    Int16 = 0x29,
    Int24 = 0x2A,
    Int32 = 0x2B,
    Int40 = 0x2C,
    Int48 = 0x2D,
    Int56 = 0x2E,
    Int64 = 0x2F,
    Enum8 = 0x30,
    Enum16 = 0x31,
    SemiPrecision = 0x38,
    SinglePrecision = 0x39,
    DoublePrecision = 0x3A,
    OctetString = 0x41,
    CharacterString = 0x42,
    LongOctetString = 0x43,
    LongCharacterString = 0x44,
    Array = 0x48,
    Structure = 0x4C,
    Set = 0x50,
    Bag = 0x51,
    TimeOfDay = 0xE0,
    Date = 0xE1,
    UtcTime = 0xE2,
    ClusterId = 0xE8,
    AttributeId = 0xE9,
    BacnetOid = 0xEA,
    IeeeAddress = 0xF0,
    SecurityKey = 0xF1,
    Unknown = 0xFF,
}

impl TypeId {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn from_u8(v: u8) -> Self {
        match v {
            0x00 => Self::NoData,
            0x08 => Self::Data8,
            0x09 => Self::Data16,
            0x0A => Self::Data24,
            0x0B => Self::Data32,
            0x0C => Self::Data40,
            0x0D => Self::Data48,
            0x0E => Self::Data56,
            0x0F => Self::Data64,
            0x10 => Self::Boolean,
            0x18 => Self::Bitmap8,
            0x19 => Self::Bitmap16,
            0x1A => Self::Bitmap24,
            0x1B => Self::Bitmap32,
            0x1C => Self::Bitmap40,
            0x1D => Self::Bitmap48,
            0x1E => Self::Bitmap56,
            0x1F => Self::Bitmap64,
            0x20 => Self::Uint8,
            0x21 => Self::Uint16,
            0x22 => Self::Uint24,
            0x23 => Self::Uint32,
            0x24 => Self::Uint40,
            0x25 => Self::Uint48,
            0x26 => Self::Uint56,
            0x27 => Self::Uint64,
            0x28 => Self::Int8,
            0x29 => Self::Int16,
            0x2A => Self::Int24,
            0x2B => Self::Int32,
            0x2C => Self::Int40,
            0x2D => Self::Int48,
            0x2E => Self::Int56,
            0x2F => Self::Int64,
            0x30 => Self::Enum8,
            0x31 => Self::Enum16,
            0x38 => Self::SemiPrecision,
            0x39 => Self::SinglePrecision,
            0x3A => Self::DoublePrecision,
            0x41 => Self::OctetString,
            0x42 => Self::CharacterString,
            0x43 => Self::LongOctetString,
            0x44 => Self::LongCharacterString,
            0x48 => Self::Array,
            0x4C => Self::Structure,
            0x50 => Self::Set,
            0x51 => Self::Bag,
            0xE0 => Self::TimeOfDay,
            0xE1 => Self::Date,
            0xE2 => Self::UtcTime,
            0xE8 => Self::ClusterId,
            0xE9 => Self::AttributeId,
            0xEA => Self::BacnetOid,
            0xF0 => Self::IeeeAddress,
            0xF1 => Self::SecurityKey,
            _ => Self::Unknown,
        }
    }

    /// Fixed wire size in bytes, or `None` for variable-length types.
    pub const fn fixed_size(self) -> Option<usize> {
        match self {
            Self::NoData => Some(0),
            Self::Boolean
            | Self::Data8
            | Self::Bitmap8
            | Self::Uint8
            | Self::Int8
            | Self::Enum8 => Some(1),
            Self::Data16
            | Self::Bitmap16
            | Self::Uint16
            | Self::Int16
            | Self::Enum16
            | Self::SemiPrecision
            | Self::ClusterId
            | Self::AttributeId => Some(2),
            Self::Data24 | Self::Bitmap24 | Self::Uint24 | Self::Int24 => Some(3),
            Self::Data32
            | Self::Bitmap32
            | Self::Uint32
            | Self::Int32
            | Self::SinglePrecision
            | Self::TimeOfDay
            | Self::Date
            | Self::UtcTime
            | Self::BacnetOid => Some(4),
            Self::Data40 | Self::Bitmap40 | Self::Uint40 | Self::Int40 => Some(5),
            Self::Data48 | Self::Bitmap48 | Self::Uint48 | Self::Int48 => Some(6),
            Self::Data56 | Self::Bitmap56 | Self::Uint56 | Self::Int56 => Some(7),
            Self::Data64
            | Self::Bitmap64
            | Self::Uint64
            | Self::Int64
            | Self::DoublePrecision
            | Self::IeeeAddress => Some(8),
            Self::SecurityKey => Some(16),
            _ => None,
        }
    }

    /// True when every byte sequence of `fixed_size()` bytes is structurally
    /// valid for this `TypeId`. Dynamic collection scanning uses this to keep a
    /// length-only fast path for unrestricted fixed-width element types while
    /// still validating null sentinels and restricted bit patterns.
    ///
    /// Must agree with `ZclSchema::ALL_PATTERNS_VALID` for every schema impl
    /// whose `TYPE_ID` maps to this variant. There is no compile-time
    /// enforcement; keep them in sync when adding new schema impls.
    pub const fn all_patterns_valid(self) -> bool {
        matches!(
            self,
            Self::Data8
                | Self::Data16
                | Self::Data24
                | Self::Data32
                | Self::Data40
                | Self::Data48
                | Self::Data56
                | Self::Data64
                | Self::Bitmap8
                | Self::Bitmap16
                | Self::Bitmap24
                | Self::Bitmap32
                | Self::Bitmap40
                | Self::Bitmap48
                | Self::Bitmap56
                | Self::Bitmap64
                | Self::SemiPrecision
                | Self::TimeOfDay
                | Self::Date
                | Self::UtcTime
                | Self::ClusterId
                | Self::AttributeId
                | Self::BacnetOid
                | Self::IeeeAddress
                | Self::SecurityKey
        )
    }
}

/// Raw ZCL type identifier byte for nested dynamic positions.
///
/// `TypeId::from_u8` intentionally maps unknown bytes to `TypeId::Unknown`.
/// Dynamic collection and structure views use this wrapper when the original
/// wire byte must be preserved for diagnostics or forwarding.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawTypeId(u8);

impl RawTypeId {
    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn from_type_id(type_id: TypeId) -> Self {
        Self(type_id.as_u8())
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn known(self) -> Option<TypeId> {
        match TypeId::from_u8(self.0) {
            TypeId::Unknown => None,
            known => Some(known),
        }
    }
}

impl From<TypeId> for RawTypeId {
    fn from(type_id: TypeId) -> Self {
        Self::from_type_id(type_id)
    }
}

impl From<u8> for RawTypeId {
    fn from(raw: u8) -> Self {
        Self::new(raw)
    }
}

impl PartialEq<TypeId> for RawTypeId {
    fn eq(&self, other: &TypeId) -> bool {
        match other {
            TypeId::Unknown => self.known().is_none(),
            known => self.known() == Some(*known),
        }
    }
}

impl PartialEq<RawTypeId> for TypeId {
    fn eq(&self, other: &RawTypeId) -> bool {
        other == self
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClusterId(pub u16);

impl ClusterId {
    pub const fn new(val: u16) -> Self {
        Self(val)
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttributeId(pub u16);

impl AttributeId {
    pub const fn new(val: u16) -> Self {
        Self(val)
    }
}

/// ZCL cluster-specific command identifier (see ZCL spec 2.4.1).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandId(pub u8);

impl CommandId {
    pub const fn new(val: u8) -> Self {
        Self(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_raw_type_id_compares_equal_to_type_id_unknown() {
        assert_eq!(RawTypeId::new(0xFE), TypeId::Unknown);
        assert_eq!(TypeId::Unknown, RawTypeId::new(0xFE));
    }

    #[test]
    fn known_raw_type_id_compares_to_matching_type_id() {
        assert_eq!(RawTypeId::new(0x21), TypeId::Uint16);
        assert_ne!(RawTypeId::new(0x21), TypeId::Uint8);
    }
}
