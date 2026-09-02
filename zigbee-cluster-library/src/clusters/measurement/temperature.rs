//! Temperature Measurement Cluster
//!
//! See Section 4.4
//!
//! Provides an interface to temperature measurement functionality, including
//! configuration and provision of notifications of temperature measurements.

use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Int16;
use crate::types::integers::Uint16;

/// Cluster descriptor (ZCL 4.4).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0402), "TemperatureMeasurement");

/// Cluster identifier (ZCL 4.4), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// `MeasuredValue`, hundredths of a degree Celsius (ZCL 4.4.2.2.1.1).
pub const MEASURED_VALUE: Attribute<Int16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "MeasuredValue");

/// `MinMeasuredValue`, the lowest value `MeasuredValue` can take
/// (ZCL 4.4.2.2.1.2).
pub const MIN_MEASURED_VALUE: Attribute<Int16> =
    CLUSTER.attribute(AttributeId(0x0001), "MinMeasuredValue");

/// `MaxMeasuredValue`, the highest value `MeasuredValue` can take
/// (ZCL 4.4.2.2.1.3).
pub const MAX_MEASURED_VALUE: Attribute<Int16> =
    CLUSTER.attribute(AttributeId(0x0002), "MaxMeasuredValue");

/// `Tolerance`, the magnitude of the measurement error (ZCL 4.4.2.2.1.4).
pub const TOLERANCE: Attribute<Uint16> = CLUSTER.attribute(AttributeId(0x0003), "Tolerance");

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptors above, which stay the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `MeasuredValue` (`Int16`, hundredths of a degree Celsius).
    pub const MEASURED_VALUE: u16 = super::MEASURED_VALUE.id().0;
    /// `MinMeasuredValue` (`Int16`).
    pub const MIN_MEASURED_VALUE: u16 = super::MIN_MEASURED_VALUE.id().0;
    /// `MaxMeasuredValue` (`Int16`).
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

    #[test]
    fn descriptors_carry_the_spec_types() {
        // 4.4.2.2.1: MeasuredValue is int16, Tolerance is uint16
        assert_eq!(MEASURED_VALUE_TYPE, TypeId::Int16);
        assert_eq!(TOLERANCE.type_id(), TypeId::Uint16);
        assert_eq!(MEASURED_VALUE.id().0, 0x0000);
        assert_eq!(MEASURED_VALUE.cluster().id(), ClusterId(CLUSTER_ID));
    }

    // 4.4.2.2.1.1: MeasuredValue = 100 x the temperature in degrees Celsius
    #[test]
    fn report_record_round_trips() {
        let mut out = [0u8; 16];
        let offset = &mut 0;
        MEASURED_VALUE
            .report(Int16(2300), &mut out, offset)
            .expect("report encoded");

        // attribute identifier | type int16 | 2300 little endian
        assert_eq!(&out[..*offset], &[0x00, 0x00, 0x29, 0xfc, 0x08]);

        let read = &mut 3;
        let value = MEASURED_VALUE
            .decode(TypeId::Int16, &out[..*offset], read)
            .expect("value decoded");
        assert_eq!(value, Int16(2300));
    }

    #[test]
    fn a_mismatched_type_identifier_is_rejected() {
        let bytes = [0x00u8, 0x00];
        let offset = &mut 0;
        assert!(
            MEASURED_VALUE
                .decode(TypeId::Uint16, &bytes, offset)
                .is_err()
        );
    }
}
