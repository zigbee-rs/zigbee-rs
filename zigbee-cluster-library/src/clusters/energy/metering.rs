//! Metering Cluster
//!
//! See Section 10.4
//!
//! Provides a remote reading of an electricity, gas, water, heat or cooling
//! meter: the running total the meter has delivered, the unit that total is
//! counted in, how it is to be formatted, and the error conditions the meter
//! currently sees.
//!
//! The meter counts in its own raw units — one pulse of a pulse output, say —
//! and publishes `Multiplier` and `Divisor` so the reader converts that count
//! into the unit `UnitofMeasure` names (10.4.2.2.4.2/10.4.2.2.4.3):
//!
//! ```text
//! reading = CurrentSummationDelivered x Multiplier / Divisor  [UnitofMeasure]
//! ```
//!
//! A water meter with a one-litre pulse output reporting cubic metres is
//! therefore `Multiplier = 1`, `Divisor = 1000`.

use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use spin::Mutex;
use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::reporting::AttributeReporting;
use crate::server::ClusterServer;
use crate::types::bitmaps::Bitmap8;
use crate::types::bitmaps::ZclBitmap8;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::enums::Enum8;
use crate::types::enums::ZclEnum8;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Uint24;
use crate::types::integers::Uint48;

/// Cluster descriptor (ZCL 10.4).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0702), "Metering");

/// Cluster identifier (ZCL 10.4), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// Largest summation this cluster can carry.
///
/// `uint48` reserves the all-ones pattern as its non-value (2.6.2.4), so the
/// top of the range the attribute table gives is one below `0xffffffffffff`.
pub const MAX_SUMMATION: u64 = 0x0000_ffff_ffff_fffe;

/// Largest `Multiplier` or `Divisor` this cluster can carry (`uint24`, with
/// the non-value excluded).
pub const MAX_SCALING: u32 = 0x00ff_fffe;

/// `CurrentSummationDelivered`, the running total the meter has delivered, in
/// raw meter units (ZCL 10.4.2.2.1.1).
pub const CURRENT_SUMMATION_DELIVERED: Attribute<Uint48, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "CurrentSummationDelivered");

/// `Status`, the error conditions the meter currently sees
/// (ZCL 10.4.2.2.3.1).
pub const STATUS: Attribute<Bitmap8<MeterStatus>, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0200), "Status");

/// `UnitofMeasure`, the unit every summation and demand of this cluster is
/// expressed in (ZCL 10.4.2.2.4.1).
pub const UNIT_OF_MEASURE: Attribute<Enum8<UnitOfMeasure>> =
    CLUSTER.attribute(AttributeId(0x0300), "UnitofMeasure");

/// `Multiplier`, the numerator converting a raw meter count into
/// `UnitofMeasure` (ZCL 10.4.2.2.4.2).
pub const MULTIPLIER: Attribute<Uint24> = CLUSTER.attribute(AttributeId(0x0301), "Multiplier");

/// `Divisor`, the denominator converting a raw meter count into
/// `UnitofMeasure` (ZCL 10.4.2.2.4.3).
pub const DIVISOR: Attribute<Uint24> = CLUSTER.attribute(AttributeId(0x0302), "Divisor");

/// `SummationFormatting`, how a summation is to be displayed
/// (ZCL 10.4.2.2.4.4).
pub const SUMMATION_FORMATTING: Attribute<Bitmap8<SummationFormatting>> =
    CLUSTER.attribute(AttributeId(0x0303), "SummationFormatting");

/// `MeteringDeviceType`, the commodity this meter measures
/// (ZCL 10.4.2.2.4.7).
pub const METERING_DEVICE_TYPE: Attribute<Bitmap8<MeteringDeviceType>> =
    CLUSTER.attribute(AttributeId(0x0306), "MeteringDeviceType");

/// The mandatory attributes, in ascending identifier order (2.5.13.3).
const ATTRIBUTES: &[AttrInfo] = &[
    CURRENT_SUMMATION_DELIVERED.attr_info(),
    STATUS.attr_info(),
    UNIT_OF_MEASURE.attr_info(),
    SUMMATION_FORMATTING.attr_info(),
    METERING_DEVICE_TYPE.attr_info(),
];

/// The mandatory attributes plus `Multiplier` and `Divisor`, in ascending
/// identifier order.
///
/// The two sit between the mandatory identifiers, so a server that publishes
/// them advertises this list rather than a prefix of the other.
const ATTRIBUTES_SCALED: &[AttrInfo] = &[
    CURRENT_SUMMATION_DELIVERED.attr_info(),
    STATUS.attr_info(),
    UNIT_OF_MEASURE.attr_info(),
    MULTIPLIER.attr_info(),
    DIVISOR.attr_info(),
    SUMMATION_FORMATTING.attr_info(),
    METERING_DEVICE_TYPE.attr_info(),
];

/// The commodity a meter measures (ZCL 10.4.2.2.4.7, Table 10-73).
///
/// The attribute table types this `map8`, but the values enumerate rather than
/// combine; the specification says so itself and keeps the wire type only for
/// backwards compatibility. It is modelled here as the wire type, so an
/// unnamed value survives a read instead of failing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeteringDeviceType(u8);

impl MeteringDeviceType {
    pub const ELECTRIC: Self = Self(0);
    pub const GAS: Self = Self(1);
    pub const WATER: Self = Self(2);
    /// Deprecated by the specification; kept because the value is assigned.
    pub const THERMAL: Self = Self(3);
    pub const PRESSURE: Self = Self(4);
    pub const HEAT: Self = Self(5);
    pub const COOLING: Self = Self(6);
    /// End Use Measurement Device for electric vehicle charging.
    pub const EUMD_EV_CHARGING: Self = Self(7);
    pub const PV_GENERATION: Self = Self(8);
    pub const WIND_TURBINE_GENERATION: Self = Self(9);
    pub const WATER_TURBINE_GENERATION: Self = Self(10);
    pub const MICRO_GENERATION: Self = Self(11);
    pub const SOLAR_HOT_WATER_GENERATION: Self = Self(12);
    pub const ELECTRIC_PHASE_1: Self = Self(13);
    pub const ELECTRIC_PHASE_2: Self = Self(14);
    pub const ELECTRIC_PHASE_3: Self = Self(15);

    /// Offset from a metering device type to the mirrored one a mirror
    /// provided for a battery-powered meter takes on (ZCL 10.4.2.2.4.7).
    pub const MIRRORED_OFFSET: u8 = 127;

    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// The mirrored device type a mirror of this meter takes on, `None` for a
    /// type that is already mirrored or has no mirrored counterpart.
    pub const fn mirrored(self) -> Option<Self> {
        if self.0 > 15 {
            return None;
        }
        Some(Self(self.0 + Self::MIRRORED_OFFSET))
    }
}

impl ZclBitmap8 for MeteringDeviceType {
    fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    fn into_bits(self) -> u8 {
        self.0
    }
}

/// The base unit a metering cluster counts in (ZCL 10.4.2.2.4.1,
/// Table 10-72).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Unit {
    /// kWh and kW.
    KilowattHours = 0x00,
    /// m³ and m³/h.
    CubicMetres = 0x01,
    /// ft³ and ft³/h.
    CubicFeet = 0x02,
    /// ccf (centum cubic feet) and ccf/h.
    CentumCubicFeet = 0x03,
    /// US gallons and US gl/h.
    UsGallons = 0x04,
    /// Imperial gallons and IMP gl/h.
    ImperialGallons = 0x05,
    /// BTU and BTU/h.
    Btu = 0x06,
    /// Litres and l/h.
    Litres = 0x07,
    /// kPa, gauge.
    KilopascalGauge = 0x08,
    /// kPa, absolute.
    KilopascalAbsolute = 0x09,
    /// mcf (1000 cubic feet) and mcf/h.
    ThousandCubicFeet = 0x0A,
    Unitless = 0x0B,
    /// MJ and MJ/s.
    Megajoule = 0x0C,
    /// kVar and kVarh.
    Kilovar = 0x0D,
}

impl Unit {
    const fn from_raw(raw: u8) -> Option<Self> {
        Some(match raw {
            0x00 => Self::KilowattHours,
            0x01 => Self::CubicMetres,
            0x02 => Self::CubicFeet,
            0x03 => Self::CentumCubicFeet,
            0x04 => Self::UsGallons,
            0x05 => Self::ImperialGallons,
            0x06 => Self::Btu,
            0x07 => Self::Litres,
            0x08 => Self::KilopascalGauge,
            0x09 => Self::KilopascalAbsolute,
            0x0A => Self::ThousandCubicFeet,
            0x0B => Self::Unitless,
            0x0C => Self::Megajoule,
            0x0D => Self::Kilovar,
            _ => return None,
        })
    }
}

/// `UnitofMeasure` (ZCL 10.4.2.2.4.1).
///
/// Each unit appears twice in the enumeration: once for a meter whose
/// registers are pure binary, and once, with bit 7 set, for one whose
/// registers are binary-coded decimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitOfMeasure {
    unit: Unit,
    bcd: bool,
}

impl UnitOfMeasure {
    /// Bit 7 of the enumeration, set for the BCD half of the table.
    const BCD: u8 = 0x80;

    /// A unit counted in pure binary, which is what a meter reporting over ZCL
    /// normally uses.
    pub const fn binary(unit: Unit) -> Self {
        Self { unit, bcd: false }
    }

    /// A unit whose registers are binary-coded decimal (ZCL 10.4.2.2.4.1).
    pub const fn bcd(unit: Unit) -> Self {
        Self { unit, bcd: true }
    }

    /// The base unit, without the number format.
    pub const fn unit(self) -> Unit {
        self.unit
    }

    /// Whether the registers are binary-coded decimal rather than pure binary.
    pub const fn is_bcd(self) -> bool {
        self.bcd
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        if self.bcd {
            return self.unit as u8 | Self::BCD;
        }
        self.unit as u8
    }
}

impl ZclEnum8 for UnitOfMeasure {
    fn from_raw(raw: u8) -> Option<Self> {
        let bcd = raw & Self::BCD != 0;
        let unit = Unit::from_raw(raw & !Self::BCD)?;
        Some(Self { unit, bcd })
    }

    fn into_raw(self) -> u8 {
        self.raw()
    }
}

/// `SummationFormatting`, how a summation is to be rendered
/// (ZCL 10.4.2.2.4.4).
///
/// This says nothing about the value on the wire — the wire value is always
/// scaled by `Multiplier` and `Divisor`. It tells a display how many digits to
/// give the result on either side of the decimal point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SummationFormatting(u8);

impl SummationFormatting {
    /// Bits 0 to 2: digits to the right of the decimal point.
    const RIGHT: u8 = 0b0000_0111;
    /// Bits 3 to 6: digits to the left of the decimal point.
    const LEFT: u8 = 0b0111_1000;
    const LEFT_SHIFT: u8 = 3;
    /// Bit 7: suppress leading zeros.
    const SUPPRESS_LEADING_ZEROS: u8 = 0b1000_0000;

    /// Digits either side of the decimal point, saturating at what the field
    /// can hold: 7 to the right, 15 to the left.
    pub const fn new(digits_right: u8, digits_left: u8, suppress_leading_zeros: bool) -> Self {
        let right = if digits_right > 7 { 7 } else { digits_right };
        let left = if digits_left > 15 { 15 } else { digits_left };
        let mut bits = right | (left << Self::LEFT_SHIFT);
        if suppress_leading_zeros {
            bits |= Self::SUPPRESS_LEADING_ZEROS;
        }
        Self(bits)
    }

    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Digits to the right of the decimal point.
    pub const fn digits_right(self) -> u8 {
        self.0 & Self::RIGHT
    }

    /// Digits to the left of the decimal point.
    pub const fn digits_left(self) -> u8 {
        (self.0 & Self::LEFT) >> Self::LEFT_SHIFT
    }

    /// Whether leading zeros are to be suppressed.
    pub const fn suppresses_leading_zeros(self) -> bool {
        self.0 & Self::SUPPRESS_LEADING_ZEROS != 0
    }
}

impl ZclBitmap8 for SummationFormatting {
    fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    fn into_bits(self) -> u8 {
        self.0
    }
}

/// `Status`, the error and warning conditions the meter currently sees
/// (ZCL 10.4.2.2.3.1).
///
/// Bits 0 to 2 and bits 5 and 6 mean the same thing for every commodity; bits
/// 3, 4 and 7 are read against the meter's own `MeteringDeviceType`, which is
/// why the constants below are grouped by commodity and overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MeterStatus(u8);

impl MeterStatus {
    /// No condition set, which is the attribute's default.
    pub const OK: Self = Self(0x00);

    // Tables 10-62 through 10-65, the bits every commodity shares.

    /// A non-fatal problem was detected: a measurement, memory or self check
    /// error.
    pub const CHECK_METER: Self = Self(0x01);
    /// The battery needs maintenance.
    pub const LOW_BATTERY: Self = Self(0x02);
    /// A tamper event was detected.
    pub const TAMPER_DETECT: Self = Self(0x04);
    /// A leak was detected.
    pub const LEAK_DETECT: Self = Self(0x20);
    /// The service to the premises has been disconnected.
    pub const SERVICE_DISCONNECT: Self = Self(0x40);

    // Table 10-64, water. Gas (Table 10-63) shares all but `PIPE_EMPTY`.

    /// Water and gas: the service pipe is empty, with no flow either way.
    pub const PIPE_EMPTY: Self = Self(0x08);
    /// Water and gas: the pressure is below the meter's threshold.
    pub const LOW_PRESSURE: Self = Self(0x10);
    /// Water and gas: flow was detected from consumer to supplier.
    pub const REVERSE_FLOW: Self = Self(0x80);

    // Table 10-62, electricity.

    /// Electricity: a power outage is in progress.
    pub const POWER_FAILURE: Self = Self(0x08);
    /// Electricity: a power quality event, such as low or high voltage.
    pub const POWER_QUALITY: Self = Self(0x10);

    // Table 10-65, heat and cooling.

    /// Heat and cooling: a temperature sensor reports an error.
    pub const TEMPERATURE_SENSOR: Self = Self(0x08);
    /// Heat and cooling: a burst was detected on the premises' pipes.
    pub const BURST_DETECT: Self = Self(0x10);
    /// Heat and cooling: a flow sensor reports an error.
    pub const FLOW_SENSOR: Self = Self(0x80);

    /// Wraps a raw attribute value.
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// The value as it appears on the wire.
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Whether every condition in `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Both sets of conditions.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// This set with the conditions in `other` cleared.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl ZclBitmap8 for MeterStatus {
    fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    fn into_bits(self) -> u8 {
        self.0
    }
}

/// `Multiplier` and `Divisor`, which are only meaningful together
/// (ZCL 10.4.2.2.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scaling {
    multiplier: u32,
    divisor: u32,
}

/// Metering cluster server (ZCL 10.4).
///
/// Holds the mandatory attributes and answers reads and discovery for them.
/// The application owns the register: it pushes each new total in with
/// [`set_current_summation_delivered`](Self::set_current_summation_delivered),
/// or counts up from a pulse input with
/// [`add_summation_delivered`](Self::add_summation_delivered), and emits the
/// reports itself.
///
/// Whether a coordinator may configure those reports is
/// [`with_reporting`](Self::with_reporting): given a store, `Configure
/// Reporting` is accepted and the application follows the intervals it
/// accepted; without one, the reporting configuration commands are refused and
/// the device alone decides when to report.
///
/// A water meter on a one-litre pulse output, reporting cubic metres to three
/// decimal places:
///
/// ```
/// use zigbee_cluster_library::clusters::energy::metering::MeteringDeviceType;
/// use zigbee_cluster_library::clusters::energy::metering::MeteringServer;
/// use zigbee_cluster_library::clusters::energy::metering::SummationFormatting;
/// use zigbee_cluster_library::clusters::energy::metering::Unit;
/// use zigbee_cluster_library::clusters::energy::metering::UnitOfMeasure;
///
/// let meter = MeteringServer::new(
///     MeteringDeviceType::WATER,
///     UnitOfMeasure::binary(Unit::CubicMetres),
///     SummationFormatting::new(3, 6, true),
/// )
/// .with_scaling(1, 1000);
///
/// // one pulse per litre
/// meter.add_summation_delivered(1);
/// assert_eq!(meter.current_summation_delivered(), 1);
/// ```
pub struct MeteringServer<'a> {
    // not an `AtomicU64`: the 32-bit targets this runs on have no 64-bit
    // atomic, and the register is read and written far too rarely for the lock
    // to matter
    summation: Mutex<u64>,
    status: AtomicU8,
    device_type: MeteringDeviceType,
    unit: UnitOfMeasure,
    formatting: SummationFormatting,
    scaling: Option<Scaling>,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> MeteringServer<'a> {
    /// A meter of `device_type`, counting in `unit` and displayed as
    /// `formatting` says (ZCL 10.4.2.2.4).
    ///
    /// `CurrentSummationDelivered` starts at zero and `Status` at
    /// [`MeterStatus::OK`]. Without
    /// [`with_scaling`](Self::with_scaling) the raw count is the reading, as
    /// an absent `Multiplier`/`Divisor` pair means a factor of one.
    pub const fn new(
        device_type: MeteringDeviceType,
        unit: UnitOfMeasure,
        formatting: SummationFormatting,
    ) -> Self {
        Self {
            summation: Mutex::new(0),
            status: AtomicU8::new(MeterStatus::OK.raw()),
            device_type,
            unit,
            formatting,
            scaling: None,
            reporting: None,
        }
    }

    /// Publish the `Multiplier` and `Divisor` converting the raw count into
    /// [`UnitOfMeasure`] (ZCL 10.4.2.2.4.2/10.4.2.2.4.3).
    ///
    /// Both are optional attributes, but a meter counting in anything other
    /// than whole units of measure — a pulse output, say — is unreadable
    /// without them.
    ///
    /// # Panics
    ///
    /// When `divisor` is zero, or either value exceeds [`MAX_SCALING`]: a
    /// coordinator cannot be handed a factor it cannot apply.
    #[must_use]
    pub const fn with_scaling(mut self, multiplier: u32, divisor: u32) -> Self {
        assert!(divisor != 0, "Divisor must not be zero");
        assert!(
            multiplier <= MAX_SCALING && divisor <= MAX_SCALING,
            "Multiplier and Divisor are uint24"
        );
        self.scaling = Some(Scaling {
            multiplier,
            divisor,
        });
        self
    }

    /// Let a coordinator configure reporting, keeping what it asks for in
    /// `store` (ZCL 2.5.7).
    ///
    /// `CurrentSummationDelivered` is the one value a metering device exists
    /// to publish, so a server that leaves this unset refuses `Configure
    /// Reporting` with `UNREPORTABLE_ATTRIBUTE` and a strict coordinator will
    /// call that a failed interview step.
    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    /// The commodity this meter measures.
    pub const fn device_type(&self) -> MeteringDeviceType {
        self.device_type
    }

    /// The unit the reading is expressed in.
    pub const fn unit_of_measure(&self) -> UnitOfMeasure {
        self.unit
    }

    /// How the reading is to be displayed.
    pub const fn summation_formatting(&self) -> SummationFormatting {
        self.formatting
    }

    /// `Multiplier` and `Divisor`, if this server publishes them.
    pub const fn scaling(&self) -> Option<(u32, u32)> {
        match self.scaling {
            Some(Scaling {
                multiplier,
                divisor,
            }) => Some((multiplier, divisor)),
            None => None,
        }
    }

    /// The running total, in raw meter units.
    pub fn current_summation_delivered(&self) -> u64 {
        *self.summation.lock()
    }

    /// Record a new running total, clamped to [`MAX_SUMMATION`]
    /// (ZCL 10.4.2.2.1.1).
    pub fn set_current_summation_delivered(&self, value: u64) {
        *self.summation.lock() = value.min(MAX_SUMMATION);
    }

    /// Count `count` raw units onto the running total, saturating at
    /// [`MAX_SUMMATION`].
    ///
    /// This is what a pulse input drives: the summation only ever grows, and a
    /// meter that would roll over stays at its top value rather than reading
    /// as though the premises had consumed nothing.
    pub fn add_summation_delivered(&self, count: u64) {
        let mut summation = self.summation.lock();
        *summation = summation.saturating_add(count).min(MAX_SUMMATION);
    }

    /// The error and warning conditions the meter currently reports.
    pub fn status(&self) -> MeterStatus {
        MeterStatus::from_raw(self.status.load(Ordering::Relaxed))
    }

    /// Replace the reported conditions wholesale.
    pub fn set_status(&self, status: MeterStatus) {
        self.status.store(status.raw(), Ordering::Relaxed);
    }

    /// Raise the conditions in `status`, leaving the others as they are.
    pub fn raise_status(&self, status: MeterStatus) {
        self.status.fetch_or(status.raw(), Ordering::Relaxed);
    }

    /// Clear the conditions in `status`, leaving the others as they are.
    pub fn clear_status(&self, status: MeterStatus) {
        self.status.fetch_and(!status.raw(), Ordering::Relaxed);
    }
}

impl ClusterServer for MeteringServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        if self.scaling.is_some() {
            ATTRIBUTES_SCALED
        } else {
            // Multiplier and Divisor are optional (ZCL 10.4.2.2.4.2); do not
            // advertise what this server cannot answer
            ATTRIBUTES
        }
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        let encoded = match id.0 {
            attribute::CURRENT_SUMMATION_DELIVERED => CURRENT_SUMMATION_DELIVERED.encode(
                Uint48(self.current_summation_delivered()),
                out,
                offset,
            ),
            attribute::STATUS => STATUS.encode(self.status(), out, offset),
            attribute::UNIT_OF_MEASURE => UNIT_OF_MEASURE.encode(self.unit, out, offset),
            attribute::MULTIPLIER => {
                let Some(scaling) = self.scaling else {
                    return Status::UnsupportedAttribute;
                };
                MULTIPLIER.encode(Uint24(scaling.multiplier), out, offset)
            }
            attribute::DIVISOR => {
                let Some(scaling) = self.scaling else {
                    return Status::UnsupportedAttribute;
                };
                DIVISOR.encode(Uint24(scaling.divisor), out, offset)
            }
            attribute::SUMMATION_FORMATTING => {
                SUMMATION_FORMATTING.encode(self.formatting, out, offset)
            }
            attribute::METERING_DEVICE_TYPE => {
                METERING_DEVICE_TYPE.encode(self.device_type, out, offset)
            }
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

impl ClusterRequestHandler for MeteringServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptors above, which stay the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `CurrentSummationDelivered` (`Uint48`, raw meter units).
    pub const CURRENT_SUMMATION_DELIVERED: u16 = super::CURRENT_SUMMATION_DELIVERED.id().0;
    /// `Status` (`Map8`).
    pub const STATUS: u16 = super::STATUS.id().0;
    /// `UnitofMeasure` (`Enum8`).
    pub const UNIT_OF_MEASURE: u16 = super::UNIT_OF_MEASURE.id().0;
    /// `Multiplier` (`Uint24`).
    pub const MULTIPLIER: u16 = super::MULTIPLIER.id().0;
    /// `Divisor` (`Uint24`).
    pub const DIVISOR: u16 = super::DIVISOR.id().0;
    /// `SummationFormatting` (`Map8`).
    pub const SUMMATION_FORMATTING: u16 = super::SUMMATION_FORMATTING.id().0;
    /// `MeteringDeviceType` (`Map8`).
    pub const METERING_DEVICE_TYPE: u16 = super::METERING_DEVICE_TYPE.id().0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::TypeId;

    // descriptors are consts, so their metadata is available at compile time
    const SUMMATION_TYPE: TypeId = CURRENT_SUMMATION_DELIVERED.type_id();

    fn water_meter<'a>() -> MeteringServer<'a> {
        MeteringServer::new(
            MeteringDeviceType::WATER,
            UnitOfMeasure::binary(Unit::CubicMetres),
            SummationFormatting::new(3, 6, false),
        )
    }

    // Table 10-57, 10-61 and 10-71: the wire types of the mandatory set
    #[test]
    fn descriptors_carry_the_spec_types() {
        assert_eq!(SUMMATION_TYPE, TypeId::Uint48);
        assert_eq!(STATUS.type_id(), TypeId::Bitmap8);
        assert_eq!(UNIT_OF_MEASURE.type_id(), TypeId::Enum8);
        assert_eq!(MULTIPLIER.type_id(), TypeId::Uint24);
        assert_eq!(DIVISOR.type_id(), TypeId::Uint24);
        assert_eq!(SUMMATION_FORMATTING.type_id(), TypeId::Bitmap8);
        assert_eq!(METERING_DEVICE_TYPE.type_id(), TypeId::Bitmap8);

        assert_eq!(CURRENT_SUMMATION_DELIVERED.id().0, 0x0000);
        assert_eq!(
            CURRENT_SUMMATION_DELIVERED.cluster().id(),
            ClusterId(CLUSTER_ID)
        );
        assert_eq!(CLUSTER_ID, 0x0702);
    }

    // 10.4.2.2.1.1: the reading is the one attribute a coordinator subscribes
    // to, so its record has to round trip
    #[test]
    fn report_record_round_trips() {
        let mut out = [0u8; 16];
        let offset = &mut 0;
        CURRENT_SUMMATION_DELIVERED
            .report(Uint48(123_456), &mut out, offset)
            .expect("report encoded");

        // attribute identifier | type uint48 | 123456 little endian
        assert_eq!(
            &out[..*offset],
            &[0x00, 0x00, 0x25, 0x40, 0xe2, 0x01, 0x00, 0x00, 0x00]
        );

        let read = &mut 3;
        let value = CURRENT_SUMMATION_DELIVERED
            .decode(TypeId::Uint48, &out[..*offset], read)
            .expect("value decoded");
        assert_eq!(value, Uint48(123_456));
    }

    // Table 10-57: the mandatory attributes are all readable, which is what an
    // interview asks for first
    #[test]
    fn the_mandatory_attributes_are_readable() {
        let server = water_meter();
        server.set_current_summation_delivered(123_456);

        // Read Attributes for CurrentSummationDelivered, Status,
        // UnitofMeasure, SummationFormatting, MeteringDeviceType
        let asdu = [
            0x00, 0x2a, 0x00, // frame control, sequence, ReadAttributes
            0x00, 0x00, 0x00, 0x02, 0x00, 0x03, 0x03, 0x03, 0x06, 0x03,
        ];
        let request = ClusterRequest {
            profile_id: 0x0104,
            cluster_id: CLUSTER_ID,
            src_endpoint: 1,
            dst_endpoint: 1,
            unicast: true,
            asdu: &asdu,
        };

        let mut out = [0u8; 64];
        let reply = server.handle(&request, &mut out).expect("handled");
        assert_eq!(
            &out[..reply.len],
            &[
                0x18, 0x2a, 0x01, // frame control, sequence, ReadAttributesResponse
                0x00, 0x00, 0x00, 0x25, 0x40, 0xe2, 0x01, 0x00, 0x00,
                0x00, // summation, uint48, 123456
                0x00, 0x02, 0x00, 0x18, 0x00, // Status, map8, no condition
                0x00, 0x03, 0x00, 0x30, 0x01, // UnitofMeasure, enum8, m3 binary
                0x03, 0x03, 0x00, 0x18, 0x33, // SummationFormatting, map8, 6.3 digits
                0x06, 0x03, 0x00, 0x18, 0x02, // MeteringDeviceType, map8, water
            ]
        );
    }

    // 10.4.2.2.4.2: Multiplier and Divisor are optional, so a server without
    // them neither answers nor advertises them
    #[test]
    fn scaling_is_advertised_only_when_it_is_published() {
        let without = water_meter();
        assert_eq!(without.attributes().len(), 5);
        assert_eq!(without.scaling(), None);

        let mut out = [0u8; 8];
        assert_eq!(
            without.encode_value(AttributeId(attribute::MULTIPLIER), &mut out, &mut 0),
            Status::UnsupportedAttribute
        );

        let with = water_meter().with_scaling(1, 1000);
        assert_eq!(with.attributes().len(), 7);
        assert_eq!(with.scaling(), Some((1, 1000)));

        let offset = &mut 0;
        assert_eq!(
            with.encode_value(AttributeId(attribute::DIVISOR), &mut out, offset),
            Status::Success
        );
        // type uint24 | 1000 little endian
        assert_eq!(&out[..*offset], &[0x22, 0xe8, 0x03, 0x00]);
    }

    // 10.4.2.2.1.1: the summation is a register that only grows, and it is a
    // uint48 whose all-ones pattern is the non-value
    #[test]
    fn the_summation_saturates_rather_than_rolling_over() {
        let server = water_meter();

        server.add_summation_delivered(3);
        server.add_summation_delivered(4);
        assert_eq!(server.current_summation_delivered(), 7);

        server.add_summation_delivered(u64::MAX);
        assert_eq!(server.current_summation_delivered(), MAX_SUMMATION);

        server.set_current_summation_delivered(u64::MAX);
        assert_eq!(server.current_summation_delivered(), MAX_SUMMATION);

        // the clamp keeps the register out of the uint48 non-value, so the
        // reading still encodes
        let mut out = [0u8; 8];
        assert_eq!(
            server.encode_value(
                AttributeId(attribute::CURRENT_SUMMATION_DELIVERED),
                &mut out,
                &mut 0
            ),
            Status::Success
        );
    }

    // 10.4.2.2.4.4: bits 0-2 right of the point, bits 3-6 left, bit 7 suppress
    #[test]
    fn summation_formatting_splits_into_its_three_fields() {
        let formatting = SummationFormatting::new(3, 6, true);
        assert_eq!(formatting.raw(), 0b1011_0011);
        assert_eq!(formatting.digits_right(), 3);
        assert_eq!(formatting.digits_left(), 6);
        assert!(formatting.suppresses_leading_zeros());

        // the fields saturate at what they can hold rather than overflowing
        // into their neighbours
        let wide = SummationFormatting::new(9, 20, false);
        assert_eq!(wide.digits_right(), 7);
        assert_eq!(wide.digits_left(), 15);
        assert_eq!(wide.raw(), 0b0111_1111);
    }

    // Table 10-72: the same unit appears twice, the BCD half with bit 7 set
    #[test]
    fn unit_of_measure_carries_the_number_format_in_bit_seven() {
        assert_eq!(UnitOfMeasure::binary(Unit::CubicMetres).raw(), 0x01);
        assert_eq!(UnitOfMeasure::bcd(Unit::CubicMetres).raw(), 0x81);
        assert_eq!(UnitOfMeasure::binary(Unit::Litres).raw(), 0x07);

        let decoded = <UnitOfMeasure as ZclEnum8>::from_raw(0x87).expect("named a member");
        assert_eq!(decoded.unit(), Unit::Litres);
        assert!(decoded.is_bcd());

        // beyond the last assigned unit in either half
        assert!(<UnitOfMeasure as ZclEnum8>::from_raw(0x0e).is_none());
        assert!(<UnitOfMeasure as ZclEnum8>::from_raw(0x8e).is_none());
    }

    // Table 10-64: the water status bits, and the shared ones around them
    #[test]
    fn status_conditions_are_raised_and_cleared_individually() {
        let server = water_meter();
        assert_eq!(server.status(), MeterStatus::OK);

        server.raise_status(MeterStatus::LOW_BATTERY);
        server.raise_status(MeterStatus::LEAK_DETECT);
        assert!(server.status().contains(MeterStatus::LOW_BATTERY));
        assert!(server.status().contains(MeterStatus::LEAK_DETECT));
        assert_eq!(server.status().raw(), 0x22);

        server.clear_status(MeterStatus::LEAK_DETECT);
        assert!(server.status().contains(MeterStatus::LOW_BATTERY));
        assert!(!server.status().contains(MeterStatus::LEAK_DETECT));

        server.set_status(MeterStatus::OK);
        assert_eq!(server.status(), MeterStatus::OK);
    }

    // 10.4.2.2.4.7: a mirror of a battery-powered meter takes the device type
    // 127 above the meter's own
    #[test]
    fn a_metering_device_type_maps_to_its_mirror() {
        assert_eq!(MeteringDeviceType::WATER.raw(), 2);
        assert_eq!(
            MeteringDeviceType::WATER.mirrored(),
            Some(MeteringDeviceType::from_raw(129))
        );
        assert_eq!(MeteringDeviceType::from_raw(129).mirrored(), None);
    }

    #[test]
    fn a_mismatched_type_identifier_is_rejected() {
        let bytes = [0x00u8; 6];
        let offset = &mut 0;
        assert!(
            CURRENT_SUMMATION_DELIVERED
                .decode(TypeId::Uint64, &bytes, offset)
                .is_err()
        );
    }
}
