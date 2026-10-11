//! Power Configuration Cluster
//!
//! See Section 3.3
//!
//! Provides an interface to determine detailed information about a device's
//! power source, and to configure the thresholds at which it raises battery
//! alarms.

use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::reporting::AttributeReporting;
use crate::server::ClusterServer;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Uint8;
use crate::types::nullable::Nullable;

/// Cluster descriptor (ZCL 3.3).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0001), "PowerConfiguration");

/// Cluster identifier (ZCL 3.3), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

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

/// `BatteryVoltage`, in units of 100 mV, `None` when unknown
/// (ZCL 3.3.2.2.3.1).
pub const BATTERY_VOLTAGE: Attribute<Nullable<Uint8>, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(attribute::BATTERY_VOLTAGE), "BatteryVoltage");

/// `BatteryPercentageRemaining`, in half percent, `None` when unknown
/// (ZCL 3.3.2.2.3.2).
pub const BATTERY_PERCENTAGE_REMAINING: Attribute<Nullable<Uint8>, ReadOnly, Reportable> = CLUSTER
    .attribute(
        AttributeId(attribute::BATTERY_PERCENTAGE_REMAINING),
        "BatteryPercentageRemaining",
    );

/// The attributes this server implements, in ascending identifier order
/// (2.5.13.3).
const ATTRIBUTES: &[AttrInfo] = &[
    BATTERY_VOLTAGE.attr_info(),
    BATTERY_PERCENTAGE_REMAINING.attr_info(),
];

/// Power Configuration cluster server (ZCL 3.3), battery half.
///
/// Every attribute of this cluster is optional, so a server implements the
/// subset its power source makes meaningful. This one serves the two a
/// battery-powered device is interviewed for — `BatteryVoltage` and
/// `BatteryPercentageRemaining` — and leaves the mains, size and alarm
/// attributes to a device that has something to say about them.
///
/// Both start out unknown and are answered as the non-value until the
/// application pushes a reading in, which is what the specification asks of a
/// reading a device does not have (ZCL 3.3.2.2.3).
///
/// Whether a coordinator may configure reporting is
/// [`with_reporting`](Self::with_reporting): given a store, `Configure
/// Reporting` is accepted and the application follows the intervals it
/// accepted; without one, the reporting configuration commands are refused and
/// the device alone decides when to report.
///
/// ```
/// use zigbee_cluster_library::clusters::general::power_configuration::BatteryPercentage;
/// use zigbee_cluster_library::clusters::general::power_configuration::BatteryVoltage;
/// use zigbee_cluster_library::clusters::general::power_configuration::PowerConfigurationServer;
///
/// let power = PowerConfigurationServer::new();
/// assert_eq!(power.battery_percentage().percent(), None);
///
/// power.set_battery_voltage(BatteryVoltage::from_raw(37)); // 3.7 V
/// power.set_battery_percentage(BatteryPercentage::from_percent(80));
/// assert_eq!(power.battery_percentage().percent(), Some(80.0));
/// ```
pub struct PowerConfigurationServer<'a> {
    battery_voltage: AtomicU8,
    battery_percentage: AtomicU8,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> PowerConfigurationServer<'a> {
    /// A server whose readings are both unknown until the application
    /// measures them.
    pub const fn new() -> Self {
        Self {
            battery_voltage: AtomicU8::new(UNKNOWN),
            battery_percentage: AtomicU8::new(UNKNOWN),
            reporting: None,
        }
    }

    /// Let a coordinator configure reporting, keeping what it asks for in
    /// `store` (ZCL 2.5.7).
    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    /// Latest battery voltage.
    pub fn battery_voltage(&self) -> BatteryVoltage {
        BatteryVoltage::from_raw(self.battery_voltage.load(Ordering::Relaxed))
    }

    /// Record a battery voltage (ZCL 3.3.2.2.3.1).
    pub fn set_battery_voltage(&self, voltage: BatteryVoltage) {
        self.battery_voltage.store(voltage.raw(), Ordering::Relaxed);
    }

    /// Latest remaining capacity.
    pub fn battery_percentage(&self) -> BatteryPercentage {
        BatteryPercentage::from_raw(self.battery_percentage.load(Ordering::Relaxed))
    }

    /// Record a remaining capacity (ZCL 3.3.2.2.3.2).
    pub fn set_battery_percentage(&self, percentage: BatteryPercentage) {
        self.battery_percentage
            .store(percentage.raw(), Ordering::Relaxed);
    }
}

impl Default for PowerConfigurationServer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// A raw reading as a nullable attribute value: the reserved `0xff` is the
/// non-value, not a measurement (ZCL 3.3.2.2.3).
const fn reading(raw: u8) -> Option<Uint8> {
    if raw == UNKNOWN {
        return None;
    }
    Some(Uint8(raw))
}

impl ClusterServer for PowerConfigurationServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        ATTRIBUTES
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        let encoded = match id.0 {
            attribute::BATTERY_VOLTAGE => {
                BATTERY_VOLTAGE.encode(reading(self.battery_voltage().raw()), out, offset)
            }
            attribute::BATTERY_PERCENTAGE_REMAINING => BATTERY_PERCENTAGE_REMAINING.encode(
                reading(self.battery_percentage().raw()),
                out,
                offset,
            ),
            _ => return Status::UnsupportedAttribute,
        };

        match encoded {
            Ok(()) => Status::Success,
            Err(_) => Status::InsufficientSpace,
        }
    }

    fn reporting(&self) -> Option<&dyn AttributeReporting> {
        self.reporting
    }
}

impl ClusterRequestHandler for PowerConfigurationServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::TypeId;

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

    // 3.3.2.2.3: both readings are uint8, and both carry a non-value
    #[test]
    fn descriptors_carry_the_spec_types() {
        assert_eq!(BATTERY_VOLTAGE.type_id(), TypeId::Uint8);
        assert_eq!(BATTERY_PERCENTAGE_REMAINING.type_id(), TypeId::Uint8);
        assert_eq!(BATTERY_VOLTAGE.id().0, 0x0020);
        assert_eq!(BATTERY_PERCENTAGE_REMAINING.id().0, 0x0021);
        assert_eq!(CLUSTER_ID, 0x0001);
    }

    // 3.3.2.2.3.2: what a coordinator reads during the interview of a
    // battery-powered device
    #[test]
    fn the_battery_readings_are_readable() {
        let server = PowerConfigurationServer::new();
        server.set_battery_voltage(BatteryVoltage::from_raw(37));
        server.set_battery_percentage(BatteryPercentage::from_percent(80));

        // Read Attributes for BatteryVoltage and BatteryPercentageRemaining
        let asdu = [0x00, 0x2a, 0x00, 0x20, 0x00, 0x21, 0x00];
        let request = ClusterRequest {
            profile_id: 0x0104,
            cluster_id: CLUSTER_ID,
            src_endpoint: 1,
            dst_endpoint: 1,
            unicast: true,
            asdu: &asdu,
        };

        let mut out = [0u8; 32];
        let reply = server.handle(&request, &mut out).expect("handled");
        assert_eq!(
            &out[..reply.len],
            &[
                0x18, 0x2a, 0x01, // frame control, sequence, ReadAttributesResponse
                0x20, 0x00, 0x00, 0x20, 0x25, // BatteryVoltage, uint8, 3.7 V
                0x21, 0x00, 0x00, 0x20, 0xa0, // BatteryPercentageRemaining, uint8, 80%
            ]
        );
    }

    // 3.3.2.2.3: a reading the device does not have is the non-value, not a
    // zero that would read as a flat battery
    #[test]
    fn an_unmeasured_reading_encodes_as_the_non_value() {
        let server = PowerConfigurationServer::new();

        let mut out = [0u8; 8];
        let offset = &mut 0;
        assert_eq!(
            server.encode_value(
                AttributeId(attribute::BATTERY_PERCENTAGE_REMAINING),
                &mut out,
                offset
            ),
            Status::Success
        );
        // type uint8 | non-value
        assert_eq!(&out[..*offset], &[0x20, 0xff]);
    }

    #[test]
    fn an_attribute_this_server_does_not_have_is_refused() {
        let server = PowerConfigurationServer::new();
        assert_eq!(
            server.encode_value(AttributeId(attribute::MAINS_VOLTAGE), &mut [0u8; 8], &mut 0),
            Status::UnsupportedAttribute
        );
    }
}
