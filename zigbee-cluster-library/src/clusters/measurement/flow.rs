//! Flow Measurement
//!
//! See Section 4.6
//!
//! Provides an interface to flow measurement functionality, including
//! configuration and provision of notifications of flow measurements.

use core::sync::atomic::AtomicU16;
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
use crate::types::integers::Uint16;

/// Cluster descriptor (ZCL 4.6).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0404), "FlowMeasurement");

/// Cluster identifier (ZCL 4.6), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// Reported when a flow measurement is unknown (ZCL 4.6.2.2.1.1).
pub const UNKNOWN: u16 = 0xffff;

/// Highest flow this cluster can express, `6553.4 m³/h` (ZCL 4.6.2.2.1.1).
pub const FULL_SCALE: u16 = 0xfffe;

/// `MeasuredValue`, tenths of a cubic metre per hour (ZCL 4.6.2.2.1.1).
pub const MEASURED_VALUE: Attribute<Uint16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "MeasuredValue");

/// `MinMeasuredValue`, the lowest value `MeasuredValue` can take
/// (ZCL 4.6.2.2.1.2).
pub const MIN_MEASURED_VALUE: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x0001), "MinMeasuredValue");

/// `MaxMeasuredValue`, the highest value `MeasuredValue` can take
/// (ZCL 4.6.2.2.1.3).
pub const MAX_MEASURED_VALUE: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x0002), "MaxMeasuredValue");

/// `Tolerance`, the magnitude of the measurement error (ZCL 4.6.2.2.1.4).
pub const TOLERANCE: Attribute<Uint16> = CLUSTER.attribute(AttributeId(0x0003), "Tolerance");

/// Every attribute this cluster implements, in ascending identifier order
/// (2.5.13.3).
const ATTRIBUTES: &[AttrInfo] = &[
    MEASURED_VALUE.attr_info(),
    MIN_MEASURED_VALUE.attr_info(),
    MAX_MEASURED_VALUE.attr_info(),
    TOLERANCE.attr_info(),
];

/// Flow Measurement cluster server (ZCL 4.6).
///
/// Holds the mandatory attributes and answers reads and discovery for them.
/// The application owns the sensor: it pushes each sample in with
/// [`set_measured_value`](Self::set_measured_value) and emits the reports
/// itself.
///
/// Whether a coordinator may configure those reports is
/// [`with_reporting`](Self::with_reporting): given a store, `Configure
/// Reporting` is accepted and the application follows the intervals it
/// accepted; without one, the reporting configuration commands are refused and
/// the device alone decides when to report.
pub struct FlowMeasurementServer<'a> {
    measured_value: AtomicU16,
    min_measured_value: u16,
    max_measured_value: u16,
    tolerance: Option<u16>,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> FlowMeasurementServer<'a> {
    /// A server measuring within `min..=max`, in tenths of a cubic metre per
    /// hour (ZCL 4.6.2.2.1).
    ///
    /// `MeasuredValue` starts at `min` until the application reports its first
    /// sample. Reporting stays on the device's own terms until
    /// [`with_reporting`](Self::with_reporting) hands over a store.
    pub const fn new(min: u16, max: u16) -> Self {
        Self {
            measured_value: AtomicU16::new(min),
            min_measured_value: min,
            max_measured_value: max,
            tolerance: None,
            reporting: None,
        }
    }

    /// Let a coordinator configure reporting, keeping what it asks for in
    /// `store` (ZCL 2.5.7).
    ///
    /// Reporting is mandatory for `MeasuredValue` (ZCL 4.6.2.2.1.1), so a
    /// server that leaves this unset refuses `Configure Reporting` with
    /// `UNREPORTABLE_ATTRIBUTE` and a strict coordinator will call that a
    /// failed interview step.
    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    /// Declare the measurement error `Tolerance` describes (ZCL 4.6.2.2.1.4).
    #[must_use]
    pub const fn with_tolerance(mut self, tolerance: u16) -> Self {
        self.tolerance = Some(tolerance);
        self
    }

    /// Latest sample, in tenths of a cubic metre per hour.
    pub fn measured_value(&self) -> u16 {
        self.measured_value.load(Ordering::Relaxed)
    }

    /// Record a new sample, clamped to the range this server was built with
    /// (ZCL 4.6.2.2.1.1).
    ///
    /// The clamp also keeps the attribute clear of [`UNKNOWN`], which the
    /// declared range cannot contain: a sample can never be mistaken for the
    /// non-value.
    pub fn set_measured_value(&self, value: u16) {
        let clamped = value.clamp(self.min_measured_value, self.max_measured_value);
        self.measured_value.store(clamped, Ordering::Relaxed);
    }
}

impl ClusterServer for FlowMeasurementServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        if self.tolerance.is_some() {
            ATTRIBUTES
        } else {
            // Tolerance is optional (ZCL 4.6.2.2.1.4); do not advertise what
            // this server cannot answer
            &ATTRIBUTES[..3]
        }
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        let encoded = match id.0 {
            attribute::MEASURED_VALUE => {
                MEASURED_VALUE.encode(Uint16(self.measured_value()), out, offset)
            }
            attribute::MIN_MEASURED_VALUE => {
                MIN_MEASURED_VALUE.encode(Uint16(self.min_measured_value), out, offset)
            }
            attribute::MAX_MEASURED_VALUE => {
                MAX_MEASURED_VALUE.encode(Uint16(self.max_measured_value), out, offset)
            }
            attribute::TOLERANCE => {
                let Some(tolerance) = self.tolerance else {
                    return Status::UnsupportedAttribute;
                };
                TOLERANCE.encode(Uint16(tolerance), out, offset)
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

impl ClusterRequestHandler for FlowMeasurementServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptors above, which stay the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `MeasuredValue` (`Uint16`, tenths of a cubic metre per hour).
    pub const MEASURED_VALUE: u16 = super::MEASURED_VALUE.id().0;
    /// `MinMeasuredValue` (`Uint16`).
    pub const MIN_MEASURED_VALUE: u16 = super::MIN_MEASURED_VALUE.id().0;
    /// `MaxMeasuredValue` (`Uint16`).
    pub const MAX_MEASURED_VALUE: u16 = super::MAX_MEASURED_VALUE.id().0;
    /// `Tolerance` (`Uint16`).
    pub const TOLERANCE: u16 = super::TOLERANCE.id().0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::TypeId;

    // descriptors are consts, so their metadata is available at compile time
    const MEASURED_VALUE_TYPE: TypeId = MEASURED_VALUE.type_id();

    // 4.6.2.2.1: every attribute of this cluster is uint16, unlike the
    // signed measurement clusters around it
    #[test]
    fn descriptors_carry_the_spec_types() {
        assert_eq!(MEASURED_VALUE_TYPE, TypeId::Uint16);
        assert_eq!(MIN_MEASURED_VALUE.type_id(), TypeId::Uint16);
        assert_eq!(MAX_MEASURED_VALUE.type_id(), TypeId::Uint16);
        assert_eq!(TOLERANCE.type_id(), TypeId::Uint16);

        assert_eq!(MEASURED_VALUE.id().0, 0x0000);
        assert_eq!(MEASURED_VALUE.cluster().id(), ClusterId(CLUSTER_ID));
        assert_eq!(CLUSTER_ID, 0x0404);
    }

    // 4.6.2.2.1.1: MeasuredValue = 10 x the flow in m³/h
    #[test]
    fn report_record_round_trips() {
        let mut out = [0u8; 16];
        let offset = &mut 0;
        MEASURED_VALUE
            .report(Uint16(1250), &mut out, offset)
            .expect("report encoded");

        // attribute identifier | type uint16 | 125.0 m³/h little endian
        assert_eq!(&out[..*offset], &[0x00, 0x00, 0x21, 0xe2, 0x04]);

        let read = &mut 3;
        let value = MEASURED_VALUE
            .decode(TypeId::Uint16, &out[..*offset], read)
            .expect("value decoded");
        assert_eq!(value, Uint16(1250));
    }

    // 4.6.2.2.1: the mandatory attributes are all readable, which is what an
    // interview asks for first
    #[test]
    fn the_mandatory_attributes_are_readable() {
        let server = FlowMeasurementServer::new(0, 10_000);
        server.set_measured_value(1250);

        // Read Attributes for MeasuredValue, MinMeasuredValue, MaxMeasuredValue
        let asdu = [0x00, 0x2a, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00];
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
                0x00, 0x00, 0x00, 0x21, 0xe2, 0x04, // MeasuredValue, uint16, 1250
                0x01, 0x00, 0x00, 0x21, 0x00, 0x00, // MinMeasuredValue, uint16, 0
                0x02, 0x00, 0x00, 0x21, 0x10, 0x27, // MaxMeasuredValue, uint16, 10000
            ]
        );
    }

    // Tolerance is optional, so a server without one neither answers nor
    // advertises it (ZCL 4.6.2.2.1.4)
    #[test]
    fn tolerance_is_advertised_only_when_it_is_known() {
        let without = FlowMeasurementServer::new(0, 10_000);
        assert_eq!(without.attributes().len(), 3);

        let with = FlowMeasurementServer::new(0, 10_000).with_tolerance(10);
        assert_eq!(with.attributes().len(), 4);
    }

    // 4.6.2.2.1.1: a sample outside the declared range cannot be stored, which
    // is also what keeps it clear of the 0xffff non-value
    #[test]
    fn samples_are_clamped_to_the_declared_range() {
        let server = FlowMeasurementServer::new(100, 10_000);

        server.set_measured_value(UNKNOWN);
        assert_eq!(server.measured_value(), 10_000);
        server.set_measured_value(0);
        assert_eq!(server.measured_value(), 100);

        let mut out = [0u8; 8];
        assert_eq!(
            server.encode_value(AttributeId(attribute::MEASURED_VALUE), &mut out, &mut 0),
            Status::Success
        );
    }

    #[test]
    fn a_mismatched_type_identifier_is_rejected() {
        let bytes = [0x00u8, 0x00];
        let offset = &mut 0;
        assert!(
            MEASURED_VALUE
                .decode(TypeId::Int16, &bytes, offset)
                .is_err()
        );
    }
}
