//! Power Configuration Cluster
//!
//! See Section 3.3
//!
//! Provides an interface to determine detailed information about a device's
//! power source, and to configure the thresholds at which it raises battery
//! alarms.

/// Cluster identifier (ZCL 3.3).
pub const CLUSTER_ID: u16 = 0x0001;

/// Reported by a battery attribute whose reading is invalid or unknown
/// (ZCL 3.3.2.2.3.1/3.3.2.2.3.2).
pub const UNKNOWN: u8 = 0xff;

/// Attribute identifiers (ZCL 3.3.2.2).
///
/// Every attribute of this cluster is optional; a device implements the subset
/// its power source makes meaningful.
pub mod attribute {
    /// `MainsVoltage` (`Uint16`, hundredths of a volt).
    pub const MAINS_VOLTAGE: u16 = 0x0000;
    /// `MainsFrequency` (`Uint8`).
    pub const MAINS_FREQUENCY: u16 = 0x0001;

    /// `BatteryVoltage` (`Uint8`, units of 100 mV).
    pub const BATTERY_VOLTAGE: u16 = 0x0020;
    /// `BatteryPercentageRemaining` (`Uint8`, half-percent units).
    pub const BATTERY_PERCENTAGE_REMAINING: u16 = 0x0021;

    /// `BatteryManufacturer` (`String`, up to 16 bytes).
    pub const BATTERY_MANUFACTURER: u16 = 0x0030;
    /// `BatterySize` (`Enum8`).
    pub const BATTERY_SIZE: u16 = 0x0031;
    /// `BatteryAHrRating` (`Uint16`).
    pub const BATTERY_A_HR_RATING: u16 = 0x0032;
    /// `BatteryQuantity` (`Uint8`).
    pub const BATTERY_QUANTITY: u16 = 0x0033;
    /// `BatteryRatedVoltage` (`Uint8`, units of 100 mV).
    pub const BATTERY_RATED_VOLTAGE: u16 = 0x0034;
    /// `BatteryAlarmMask` (`Map8`).
    pub const BATTERY_ALARM_MASK: u16 = 0x0035;

    /// `BatteryVoltageMinThreshold` (`Uint8`, units of 100 mV).
    pub const BATTERY_VOLTAGE_MIN_THRESHOLD: u16 = 0x0036;
    /// `BatteryVoltageThreshold1` (`Uint8`, units of 100 mV).
    pub const BATTERY_VOLTAGE_THRESHOLD_1: u16 = 0x0037;
    /// `BatteryVoltageThreshold2` (`Uint8`, units of 100 mV).
    pub const BATTERY_VOLTAGE_THRESHOLD_2: u16 = 0x0038;
    /// `BatteryVoltageThreshold3` (`Uint8`, units of 100 mV).
    pub const BATTERY_VOLTAGE_THRESHOLD_3: u16 = 0x0039;

    /// `BatteryPercentageMinThreshold` (`Uint8`, whole percent).
    pub const BATTERY_PERCENTAGE_MIN_THRESHOLD: u16 = 0x003a;
    /// `BatteryPercentageThreshold1` (`Uint8`, whole percent).
    pub const BATTERY_PERCENTAGE_THRESHOLD_1: u16 = 0x003b;
    /// `BatteryPercentageThreshold2` (`Uint8`, whole percent).
    pub const BATTERY_PERCENTAGE_THRESHOLD_2: u16 = 0x003c;
    /// `BatteryPercentageThreshold3` (`Uint8`, whole percent).
    pub const BATTERY_PERCENTAGE_THRESHOLD_3: u16 = 0x003d;

    /// `BatteryAlarmState` (`Map32`).
    pub const BATTERY_ALARM_STATE: u16 = 0x003e;
}

/// `BatteryPercentageRemaining` (ZCL 3.3.2.2.3.2).
///
/// The wire value counts half percent, so 100% is `0xc8` rather than `0x64`.
/// Reading it as whole percent halves every reading, which is why the
/// conversion lives here rather than at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryPercentage(u8);

impl BatteryPercentage {
    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// Builds from whole percent, saturating at 100%.
    pub const fn from_percent(percent: u8) -> Self {
        if percent >= 100 {
            return Self(200);
        }
        Self(percent * 2)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Remaining capacity in percent, `None` when the reading is unknown.
    pub const fn percent(self) -> Option<f32> {
        if self.0 == UNKNOWN {
            return None;
        }
        Some(self.0 as f32 / 2.0)
    }
}

/// `BatteryVoltage` and the voltage thresholds (ZCL 3.3.2.2.3.1).
///
/// Counted in units of 100 mV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryVoltage(u8);

impl BatteryVoltage {
    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Voltage in volts, `None` when the reading is unknown.
    pub const fn volts(self) -> Option<f32> {
        if self.0 == UNKNOWN {
            return None;
        }
        Some(self.0 as f32 / 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 3.3.2.2.3.2: 0x00 = 0%, 0x64 = 50%, 0xc8 = 100%
    #[test]
    fn battery_percentage_counts_half_percent() {
        assert_eq!(BatteryPercentage::from_raw(0x00).percent(), Some(0.0));
        assert_eq!(BatteryPercentage::from_raw(0x64).percent(), Some(50.0));
        assert_eq!(BatteryPercentage::from_raw(0xc8).percent(), Some(100.0));
        assert_eq!(BatteryPercentage::from_raw(0x45).percent(), Some(34.5));
    }

    #[test]
    fn battery_percentage_round_trips_whole_percent() {
        assert_eq!(BatteryPercentage::from_percent(50).raw(), 0x64);
        assert_eq!(BatteryPercentage::from_percent(100).raw(), 0xc8);
        // saturates rather than wrapping past the full-capacity value
        assert_eq!(BatteryPercentage::from_percent(255).raw(), 0xc8);
    }

    // 3.3.2.2.3.1: units of 100 mV
    #[test]
    fn battery_voltage_counts_hundred_millivolt() {
        assert_eq!(BatteryVoltage::from_raw(30).volts(), Some(3.0));
        assert_eq!(BatteryVoltage::from_raw(33).volts(), Some(3.3));
    }

    #[test]
    fn unknown_readings_have_no_value() {
        assert_eq!(BatteryPercentage::from_raw(UNKNOWN).percent(), None);
        assert_eq!(BatteryVoltage::from_raw(UNKNOWN).volts(), None);
    }
}
