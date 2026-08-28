//! Basic Cluster
//!
//! See ZCL Section 3.2
//!
//! Exposes device identity attributes (ZCL version, manufacturer name, model
//! identifier, power source, ...). [`BasicServer`] answers `Read Attributes`
//! requests, which a coordinator issues during interview to resolve the device
//! to its definition.

use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::attributes::AttributeSource;
use crate::attributes::handle_read_attributes;
use crate::common::data_types::EnumN;
use crate::common::data_types::UnsignedN;
use crate::common::data_types::ZclDataType;
use crate::common::data_types::ZclString;

/// Cluster identifier (ZCL 3.2).
pub const CLUSTER_ID: u16 = 0x0000;

/// Attribute identifiers (ZCL 3.2.2.2).
pub mod attribute {
    /// `ZCLVersion` (`uint8`).
    pub const ZCL_VERSION: u16 = 0x0000;
    /// `ApplicationVersion` (`uint8`).
    pub const APPLICATION_VERSION: u16 = 0x0001;
    /// `StackVersion` (`uint8`).
    pub const STACK_VERSION: u16 = 0x0002;
    /// `HWVersion` (`uint8`).
    pub const HW_VERSION: u16 = 0x0003;
    /// `ManufacturerName` (`string`).
    pub const MANUFACTURER_NAME: u16 = 0x0004;
    /// `ModelIdentifier` (`string`).
    pub const MODEL_IDENTIFIER: u16 = 0x0005;
    /// `PowerSource` (`enum8`).
    pub const POWER_SOURCE: u16 = 0x0007;
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

impl AttributeSource for BasicServer<'_> {
    fn cluster_id(&self) -> u16 {
        CLUSTER_ID
    }

    fn attribute(&self, id: u16) -> Option<ZclDataType<'_>> {
        Some(match id {
            attribute::ZCL_VERSION => ZclDataType::UnsignedInt(UnsignedN::Uint8(self.zcl_version)),
            attribute::APPLICATION_VERSION => {
                ZclDataType::UnsignedInt(UnsignedN::Uint8(self.application_version))
            }
            attribute::STACK_VERSION => {
                ZclDataType::UnsignedInt(UnsignedN::Uint8(self.stack_version))
            }
            attribute::HW_VERSION => ZclDataType::UnsignedInt(UnsignedN::Uint8(self.hw_version)),
            attribute::MANUFACTURER_NAME => {
                ZclDataType::String(ZclString::CharString(self.manufacturer_name))
            }
            attribute::MODEL_IDENTIFIER => {
                ZclDataType::String(ZclString::CharString(self.model_identifier))
            }
            attribute::POWER_SOURCE => ZclDataType::Enum(EnumN::Enum8(self.power_source)),
            _ => return None,
        })
    }
}

impl ClusterRequestHandler for BasicServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<usize> {
        handle_read_attributes(self, request.cluster_id, request.asdu, out)
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;
    use crate::common::data_types::ZclString;
    use crate::frame::GeneralCommand;
    use crate::frame::Status;
    use crate::frame::ZclFrame;
    use crate::header::command_identifier::CommandIdentifier;
    use crate::payload::ZclFramePayload;

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
        let len = SERVER
            .handle(&request(CLUSTER_ID, &asdu), &mut out)
            .expect("basic read handled");

        let (frame, _) = ZclFrame::try_read(&out[..len], ()).unwrap();
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
