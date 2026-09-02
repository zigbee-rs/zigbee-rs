//! Wind Speed Measurement Cluster
//!
//! See Section 4.12
//!
//! Provides an interface to wind speed measurement functionality, including
//! configuration and provision of notifications of wind speed measurements.

use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Uint16;

/// Cluster descriptor (ZCL 4.12).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x040b), "WindSpeedMeasurement");

/// Cluster identifier (ZCL 4.12), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// Reported when a value is unknown, and when a limit is not defined
/// (ZCL 4.12.2.1.1).
pub const UNKNOWN: u16 = 0xffff;

/// Largest value `MaxMeasuredValue` may take (ZCL 4.12.2.1.3).
pub const MAX_MEASURED_VALUE_LIMIT: u16 = 0xfffe;

/// `MeasuredValue` (`Uint16`, hundredths of a metre per second, ZCL 4.12.2.1.1).
pub const MEASURED_VALUE: Attribute<Uint16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "MeasuredValue");

/// `MinMeasuredValue`, the lowest value `MeasuredValue` can take.
pub const MIN_MEASURED_VALUE: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x0001), "MinMeasuredValue");

/// `MaxMeasuredValue`, the highest value `MeasuredValue` can take.
pub const MAX_MEASURED_VALUE: Attribute<Uint16> =
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

    // 4.12.2.1: every value in this cluster is uint16
    #[test]
    fn descriptors_carry_the_spec_types() {
        assert_eq!(MEASURED_VALUE.type_id(), TypeId::Uint16);
        assert_eq!(TOLERANCE.type_id(), TypeId::Uint16);
        assert_eq!(CLUSTER_ID, 0x040b);
    }

    // 4.12.2.1.1.1: MeasuredValue = 100 x the speed in m/s
    #[test]
    fn report_record_round_trips() {
        let mut out = [0u8; 16];
        let offset = &mut 0;
        MEASURED_VALUE
            .report(Uint16(1250), &mut out, offset)
            .expect("report encoded");
        assert_eq!(&out[..*offset], &[0x00, 0x00, 0x21, 0xe2, 0x04]);
    }

    // 4.12.2.1.1: 0xffff means unknown, and 0xfffe caps the maximum
    #[test]
    fn the_reserved_values_are_named() {
        assert_eq!(UNKNOWN, 0xffff);
        assert_eq!(MAX_MEASURED_VALUE_LIMIT, 0xfffe);
    }
}
