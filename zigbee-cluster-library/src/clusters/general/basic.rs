//! Basic Cluster
//!
//! See ZCL Section 3.2
//!
//! Exposes device identity attributes (ZCL version, manufacturer name, model
//! identifier, power source, ...). [`BasicServer`] answers `Read Attributes`
//! requests, which a coordinator issues during interview to resolve the device
//! to its definition.

use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::server::ClusterServer;
use crate::types::codec::zcl_type;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::ids::TypeId;
use crate::types::integers::Uint8;
use crate::types::strings::ShortStr;
use crate::types::strings::ShortText;

/// Cluster descriptor (ZCL 3.2).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0000), "Basic");

/// Cluster identifier (ZCL 3.2), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

zcl_type! {
    #[type_id = TypeId::Enum8]
    /// `PowerSource` (ZCL 3.2.2.2.9), e.g. `0x01` mains, `0x03` battery.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PowerSource(pub u8);
}

/// `ZCLVersion` (ZCL 3.2.2.2.1).
pub const ZCL_VERSION: Attribute<Uint8> = CLUSTER.attribute(AttributeId(0x0000), "ZCLVersion");
/// `ApplicationVersion` (ZCL 3.2.2.2.2).
pub const APPLICATION_VERSION: Attribute<Uint8> =
    CLUSTER.attribute(AttributeId(0x0001), "ApplicationVersion");
/// `StackVersion` (ZCL 3.2.2.2.3).
pub const STACK_VERSION: Attribute<Uint8> = CLUSTER.attribute(AttributeId(0x0002), "StackVersion");
/// `HWVersion` (ZCL 3.2.2.2.4).
pub const HW_VERSION: Attribute<Uint8> = CLUSTER.attribute(AttributeId(0x0003), "HWVersion");
/// `ManufacturerName` (ZCL 3.2.2.2.5).
pub const MANUFACTURER_NAME: Attribute<ShortText> =
    CLUSTER.attribute(AttributeId(0x0004), "ManufacturerName");
/// `ModelIdentifier` (ZCL 3.2.2.2.6).
pub const MODEL_IDENTIFIER: Attribute<ShortText> =
    CLUSTER.attribute(AttributeId(0x0005), "ModelIdentifier");
/// `PowerSource` (ZCL 3.2.2.2.9).
pub const POWER_SOURCE: Attribute<PowerSource, ReadOnly> =
    CLUSTER.attribute(AttributeId(0x0007), "PowerSource");

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptors above, which stay the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `ZCLVersion` (`uint8`).
    pub const ZCL_VERSION: u16 = super::ZCL_VERSION.id().0;
    /// `ApplicationVersion` (`uint8`).
    pub const APPLICATION_VERSION: u16 = super::APPLICATION_VERSION.id().0;
    /// `StackVersion` (`uint8`).
    pub const STACK_VERSION: u16 = super::STACK_VERSION.id().0;
    /// `HWVersion` (`uint8`).
    pub const HW_VERSION: u16 = super::HW_VERSION.id().0;
    /// `ManufacturerName` (`string`).
    pub const MANUFACTURER_NAME: u16 = super::MANUFACTURER_NAME.id().0;
    /// `ModelIdentifier` (`string`).
    pub const MODEL_IDENTIFIER: u16 = super::MODEL_IDENTIFIER.id().0;
    /// `PowerSource` (`enum8`).
    pub const POWER_SOURCE: u16 = super::POWER_SOURCE.id().0;
}

/// Basic cluster server holding the device identity attribute values.
#[derive(Debug, Clone, Copy)]
pub struct BasicServer<'a> {
    pub zcl_version: u8,
    pub application_version: u8,
    pub stack_version: u8,
    pub hw_version: u8,
    pub manufacturer_name: &'a str,
    pub model_identifier: &'a str,
    /// `PowerSource` enum8 (ZCL 3.2.2.2.9), e.g. `0x01` = mains, `0x03` =
    /// battery.
    pub power_source: u8,
}

impl ClusterServer for BasicServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        // every arm encodes through its descriptor, so a value can only be
        // written with the type ZCL 3.2.2.2 gives that attribute
        let encoded = match id.0 {
            attribute::ZCL_VERSION => ZCL_VERSION.encode(Uint8(self.zcl_version), out, offset),
            attribute::APPLICATION_VERSION => {
                APPLICATION_VERSION.encode(Uint8(self.application_version), out, offset)
            }
            attribute::STACK_VERSION => {
                STACK_VERSION.encode(Uint8(self.stack_version), out, offset)
            }
            attribute::HW_VERSION => HW_VERSION.encode(Uint8(self.hw_version), out, offset),
            attribute::MANUFACTURER_NAME => {
                let Some(name) = ShortStr::new(self.manufacturer_name) else {
                    return Status::InvalidValue;
                };
                MANUFACTURER_NAME.encode(name, out, offset)
            }
            attribute::MODEL_IDENTIFIER => {
                let Some(model) = ShortStr::new(self.model_identifier) else {
                    return Status::InvalidValue;
                };
                MODEL_IDENTIFIER.encode(model, out, offset)
            }
            attribute::POWER_SOURCE => {
                POWER_SOURCE.encode(PowerSource(self.power_source), out, offset)
            }
            _ => return Status::UnsupportedAttribute,
        };

        match encoded {
            Ok(()) => Status::Success,
            Err(_) => Status::InsufficientSpace,
        }
    }
}

impl ClusterRequestHandler for BasicServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;
    use crate::frame::GeneralCommand;
    use crate::frame::Status;
    use crate::frame::ZclFrame;
    use crate::frame::header::command_identifier::CommandIdentifier;
    use crate::frame::payload::ZclFramePayload;
    use crate::types::data_types::ZclDataType;
    use crate::types::data_types::ZclString;

    const SERVER: BasicServer = BasicServer {
        zcl_version: 8,
        application_version: 1,
        stack_version: 0,
        hw_version: 1,
        manufacturer_name: "zigbee-rs",
        model_identifier: "temp-1",
        power_source: 0x03,
    };

    fn request(cluster_id: u16, asdu: &[u8]) -> ClusterRequest<'_> {
        ClusterRequest {
            profile_id: 0x0104,
            cluster_id,
            src_endpoint: 1,
            dst_endpoint: 1,
            unicast: true,
            asdu,
        }
    }

    #[test]
    fn read_manufacturer_model_and_unknown() {
        // ZCL Read Attributes (global cmd 0x00, client->server) for
        // ManufacturerName (0x0004), ModelIdentifier (0x0005), unknown (0x9999)
        let asdu = [
            0x00, 0x2a, 0x00, // frame control, seq, command id (ReadAttributes)
            0x04, 0x00, // 0x0004
            0x05, 0x00, // 0x0005
            0x99, 0x99, // 0x9999
        ];

        let mut out = [0u8; 128];
        let reply = SERVER
            .handle(&request(CLUSTER_ID, &asdu), &mut out)
            .expect("basic read handled");

        let (frame, _) = ZclFrame::try_read(&out[..reply.len], ()).unwrap();
        assert_eq!(frame.header.sequence_number, 0x2a);
        assert_eq!(
            frame.header.command_identifier,
            CommandIdentifier::ReadAttributesResponse
        );

        let ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributesResponse(records)) =
            frame.payload
        else {
            panic!("expected ReadAttributesResponse");
        };
        assert_eq!(records.len(), 3);

        assert_eq!(records[0].attribute_id, 0x0004);
        assert_eq!(records[0].status, Status::Success);
        assert_eq!(
            records[0].value,
            Some(ZclDataType::String(ZclString::CharString("zigbee-rs")))
        );

        assert_eq!(records[1].attribute_id, 0x0005);
        assert_eq!(
            records[1].value,
            Some(ZclDataType::String(ZclString::CharString("temp-1")))
        );

        assert_eq!(records[2].attribute_id, 0x9999);
        assert_eq!(records[2].status, Status::UnsupportedAttribute);
        assert_eq!(records[2].value, None);
    }

    #[test]
    fn ignores_other_clusters() {
        let asdu = [0x00, 0x01, 0x00, 0x00, 0x00];
        let mut out = [0u8; 32];
        assert_eq!(SERVER.handle(&request(0x0402, &asdu), &mut out), None);
    }
}
