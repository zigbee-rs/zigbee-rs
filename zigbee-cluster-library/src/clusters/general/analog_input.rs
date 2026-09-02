//! Analog Input (Basic) Cluster
//!
//! See Section 3.14.2
//!
//! Provides an interface for reading the value of an analog measurement and
//! accessing various characteristics of that measurement. Carries a reading
//! the library has no dedicated cluster for — wind direction, for instance —
//! by pairing `PresentValue` with an `EngineeringUnits` code.

/// Cluster identifier (ZCL 3.14.2).
pub const CLUSTER_ID: u16 = 0x000c;

/// Attribute identifiers (ZCL 3.14.2.4.2, Table 3-78).
///
/// `PresentValue`, `OutOfService` and `StatusFlags` are mandatory; the rest are
/// optional. `single` attributes carry an IEEE 754 single-precision float.
pub mod attribute {
    /// `Description` (`String`).
    pub const DESCRIPTION: u16 = 0x001c;
    /// `MaxPresentValue` (`Single`).
    pub const MAX_PRESENT_VALUE: u16 = 0x0041;
    /// `MinPresentValue` (`Single`).
    pub const MIN_PRESENT_VALUE: u16 = 0x0045;
    /// `OutOfService` (`Bool`), mandatory.
    pub const OUT_OF_SERVICE: u16 = 0x0051;
    /// `PresentValue` (`Single`), mandatory and reportable.
    pub const PRESENT_VALUE: u16 = 0x0055;
    /// `Reliability` (`Enum8`).
    pub const RELIABILITY: u16 = 0x0067;
    /// `Resolution` (`Single`).
    pub const RESOLUTION: u16 = 0x006a;
    /// `StatusFlags` (`Map8`), mandatory and reportable.
    pub const STATUS_FLAGS: u16 = 0x006f;
    /// `EngineeringUnits` (`Enum16`), see ZCL 3.14.11.10.
    pub const ENGINEERING_UNITS: u16 = 0x0075;
    /// `ApplicationType` (`Uint32`).
    pub const APPLICATION_TYPE: u16 = 0x0100;
}

/// `StatusFlags` (ZCL 3.14.11.3).
///
/// Four flags describing the health of the analog sensor. The relationship
/// between individual flags is not defined by the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusFlags(u8);

impl StatusFlags {
    /// The `EventState` attribute is not NORMAL. Always clear unless the
    /// cluster implementing `EventState` sits on the same endpoint.
    pub const IN_ALARM: u8 = 0b0000_0001;
    /// `Reliability` is present and not NO FAULT DETECTED.
    pub const FAULT: u8 = 0b0000_0010;
    /// `PresentValue` and `Reliability` no longer track the physical input.
    pub const OVERRIDDEN: u8 = 0b0000_0100;
    /// `OutOfService` is TRUE.
    pub const OUT_OF_SERVICE: u8 = 0b0000_1000;

    /// Wraps a raw attribute value, discarding the reserved upper bits
    /// (the valid range is 0x00 to 0x0f).
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw & 0x0f)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn is_set(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    #[must_use]
    pub const fn with(self, flag: u8) -> Self {
        Self(self.0 | (flag & 0x0f))
    }

    pub const fn in_alarm(self) -> bool {
        self.is_set(Self::IN_ALARM)
    }

    pub const fn fault(self) -> bool {
        self.is_set(Self::FAULT)
    }

    pub const fn overridden(self) -> bool {
        self.is_set(Self::OVERRIDDEN)
    }

    pub const fn out_of_service(self) -> bool {
        self.is_set(Self::OUT_OF_SERVICE)
    }
}

/// `Reliability` (ZCL 3.14.11.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reliability {
    NoFaultDetected = 0x00,
    NoSensor = 0x01,
    OverRange = 0x02,
    UnderRange = 0x03,
    OpenLoop = 0x04,
    ShortedLoop = 0x05,
    NoOutput = 0x06,
    UnreliableOther = 0x07,
    ProcessError = 0x08,
    MultiStateFault = 0x09,
    ConfigurationError = 0x0a,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 3.14.11.3: bit 0 IN ALARM, 1 FAULT, 2 OVERRIDDEN, 3 OUT OF SERVICE
    #[test]
    fn status_flags_map_to_their_bits() {
        let flags = StatusFlags::from_raw(0b0000_1010);
        assert!(!flags.in_alarm());
        assert!(flags.fault());
        assert!(!flags.overridden());
        assert!(flags.out_of_service());
    }

    // the attribute range is 0x00 to 0x0f, so the upper nibble is reserved
    #[test]
    fn status_flags_discard_reserved_bits() {
        assert_eq!(StatusFlags::from_raw(0xff).raw(), 0x0f);
        assert_eq!(
            StatusFlags::default()
                .with(StatusFlags::FAULT)
                .with(StatusFlags::OVERRIDDEN)
                .raw(),
            0b0000_0110
        );
    }
}
