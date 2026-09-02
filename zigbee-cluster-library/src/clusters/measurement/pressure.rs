//! Pressure Measurement Cluster
//!
//! See Section 4.5
//!
//! Provides an interface to pressure measurement functionality, including
//! configuration and provision of notifications of pressure measurements.

use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Int16;
use crate::types::integers::Uint16;

/// Cluster descriptor (ZCL 4.5).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0403), "PressureMeasurement");

/// Cluster identifier (ZCL 4.5), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// `MeasuredValue` (`Int16`, kPa x 10, ZCL 4.5.2.2.1.1).
pub const MEASURED_VALUE: Attribute<Int16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "MeasuredValue");

/// `MinMeasuredValue`, the lowest value `MeasuredValue` can take.
pub const MIN_MEASURED_VALUE: Attribute<Int16> =
    CLUSTER.attribute(AttributeId(0x0001), "MinMeasuredValue");

/// `MaxMeasuredValue`, the highest value `MeasuredValue` can take.
pub const MAX_MEASURED_VALUE: Attribute<Int16> =
    CLUSTER.attribute(AttributeId(0x0002), "MaxMeasuredValue");

/// `Tolerance`, the magnitude of the measurement error.
pub const TOLERANCE: Attribute<Uint16> = CLUSTER.attribute(AttributeId(0x0003), "Tolerance");

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptors above, which stay the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `MeasuredValue`.
    pub const MEASURED_VALUE: u16 = super::MEASURED_VALUE.id().0;
    /// `MinMeasuredValue`.
    pub const MIN_MEASURED_VALUE: u16 = super::MIN_MEASURED_VALUE.id().0;
    /// `MaxMeasuredValue`.
    pub const MAX_MEASURED_VALUE: u16 = super::MAX_MEASURED_VALUE.id().0;
    /// `Tolerance`.
    pub const TOLERANCE: u16 = super::TOLERANCE.id().0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::TypeId;

    // 4.5.2.2.1: MeasuredValue and the limits are int16, Tolerance is uint16
    #[test]
    fn descriptors_carry_the_spec_types() {
        assert_eq!(MEASURED_VALUE.type_id(), TypeId::Int16);
        assert_eq!(MIN_MEASURED_VALUE.type_id(), TypeId::Int16);
        assert_eq!(TOLERANCE.type_id(), TypeId::Uint16);
        assert_eq!(CLUSTER_ID, 0x0403);
    }

    // 4.5.2.2.1.1: MeasuredValue = 10 x the pressure in kPa
    #[test]
    fn report_record_round_trips() {
        let mut out = [0u8; 16];
        let offset = &mut 0;
        MEASURED_VALUE
            .report(Int16(1013), &mut out, offset)
            .expect("report encoded");
        assert_eq!(&out[..*offset], &[0x00, 0x00, 0x29, 0xf5, 0x03]);
    }
}
