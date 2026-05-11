//! General ZCL Frame
#![allow(missing_docs)]

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::ctx;
use heapless::Vec;
use zigbee_macros::impl_byte;

use crate::common::data_types::ZclDataType;
use crate::header::ZclHeader;
use crate::payload::ZclFramePayload;

impl_byte! {
    /// ZCL Frame
    ///
    /// See Section 2.4.1
    #[derive(Debug, PartialEq)]
    pub struct ZclFrame<'a> {
        pub header: ZclHeader,
        #[ctx = &header]
        #[ctx_write = ()]
        pub payload: ZclFramePayload<'a>,
    }
}

/// Status enumeration
///
/// See Section 2.6.3
///
/// The spec notes that deprecated values should not be used in a transmitted
/// message and received values should be processed as if the value were the
/// replacement status value instead. For compatibility, the replacements will
/// be performed when serializing or deserializing from bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    /// Operation was successful.
    Success = 0x00,
    /// Operation was not successful.
    Failure = 0x01,
    /// The sender of the command does not have authorization to carry out this
    /// command.
    NotAuthorized = 0x7e,
    Reserved = 0x7f,
    /// The command appears to contain the wrong fields, as detected either by
    /// the presence of one or more invalid field entries or by there being
    /// missing fields. Command not carried out. Implementer has discretion as
    /// to whether to return this error or [`Status::InvalidField`].
    MalformedCommand = 0x80,
    /// The specified command is not supported on the device. Command not
    /// carried out.
    UnsupCommand = 0x81,
    /// ~The specified general ZCL command is not supported on the device.~
    ///
    /// Use [`Status::UnsupCommand`]
    #[deprecated(note = "Use `Status::UnsupCommand`")]
    UnsupGeneralCommand = 0x82,
    /// ~A manufacturer specific unicast, cluster specific command was received
    /// with an unknown manufacturer code, or the manufacturer code was
    /// recognized but the command is not supported.~
    ///
    /// Use [`Status::UnsupCommand`]
    #[deprecated = "Use `Status::UnsupCommand`"]
    UnsupManufClusterCommand = 0x83,
    /// ~A manufacturer specific unicast, ZCL specific command was received with
    /// an unknown manufacturer code, or the manufacturer code was recognized
    /// but the command is not supported.~
    ///
    /// Use [`Status::UnsupCommand`]
    #[deprecated = "Use `Status::UnsupCommand`"]
    UnsupManufGeneralCommand = 0x84,
    /// At least one field of the command contains an incorrect value, according
    /// to the specification the device is implemented to
    InvalidField = 0x85,
    /// The specified attribute does not exist on the device.
    UnsupportedAttribute = 0x86,
    /// Out of range error or set to a reserved value. Attribute keeps its old
    /// value.
    ///
    /// Note that an attribute value may be out of range if an attribute is
    /// related to another, e.g., with minimum and maximum attributes. See the
    /// individual attribute descriptions for specific details.
    InvalidValue = 0x87,
    /// Attempt to write a read-only attribute.
    ReadOnly = 0x88,
    /// An operation failed due to an insufficient amount of free space
    /// available.
    InsufficientSpace = 0x89,
    /// ~An attempt to create an entry in a table failed due to a duplicate
    /// entry already being present in the table.~
    ///
    /// Use [`Status::Success`]
    #[deprecated = "Use `Status::Success`"]
    DuplicateExists = 0x8a,
    /// The requested information (e.g., table entry) could not be found.
    NotFound = 0x8b,
    /// Periodic reports cannot be issued for this attribute.
    UnreportableAttribute = 0x8c,
    /// The data type given for an attribute is incorrect. Command not carried
    /// out.
    InvalidDataType = 0x8d,
    /// The selector for an attribute is incorrect.
    InvalidSelector = 0x8e,
    /// ~A request has been made to read an attribute that the requestor is not
    /// authorized to read. No action taken.~
    ///
    /// Use [`Status::NotAuthorized`]
    #[deprecated = "Use Status::NotAuthorized"]
    WriteOnly = 0x8f,
    /// ~Setting the requested values would put the device in an inconsistent
    /// state on startup. No action taken.~
    ///
    /// Use [`Status::Failure`]
    #[deprecated = "Use `Status::Failure`"]
    InconsistentStartupState = 0x90,
    /// ~An attempt has been made to write an attribute that is present but is
    /// defined using an out-of-band method and not over the air.~
    ///
    /// Use [`Status::Failure`]
    #[deprecated = "Use `Status::Failure`"]
    DefinedOutOfBand = 0x91,
    /// The supplied values (e.g., contents of table cells) are inconsistent.
    ///
    /// Never used.
    ReservedInconsistent = 0x92,
    /// ~The credentials presented by the device sending the command are not
    /// sufficient to perform this action.~
    ///
    /// Use [`Status::Failure`]
    #[deprecated = "Use `Status::Failure`"]
    ActionDenied = 0x93,
    /// The exchange was aborted due to excessive response time.
    Timeout = 0x94,
    /// Failed case when a client or a server decides to abort the upgrade
    /// process.
    Abort = 0x95,
    /// Invalid OTA upgrade image (ex. failed signature validation or signer
    /// information check or CRC check).
    InvalidImage = 0x96,
    /// Server does not have data block available yet.
    WaitForData = 0x97,
    /// No OTA upgrade image available for the client.
    NoImageAvailable = 0x98,
    /// The client still requires more OTA upgrade image files to successfully
    /// upgrade.
    RequireMoreImage = 0x99,
    /// The command has been received and is being processed.
    NotificationPending = 0x9a,
    /// ~An operation was unsuccessful due to a hardware failure.~
    ///
    /// Use [`Status::Failure`]
    #[deprecated = "Use `Status::Failure`"]
    HardwareFailure = 0xc0,
    /// ~An operation was unsuccessful due to a software failure.~
    ///
    /// Use [`Status::Failure`]
    #[deprecated = "Use `Status::Failure`"]
    SoftwareFailure = 0xc1,
    /// An error occurred during calibration.
    ///
    /// Never used.
    ReservedCalibration = 0xc2,
    /// The cluster is not supported.
    UnsupportedCluster = 0xc3,
    /// ~Limit of attribute range reached. Value is trimmed to closest limit
    /// (maximum or minimum).~
    ///
    /// Use [`Status::Success`]
    #[deprecated = "Use `Status::Success`"]
    LimitReached = 0xc4,
    Unknown,
}

impl Status {
    fn from_byte(b: u8) -> byte::Result<Self> {
        match b {
            0x00 => Ok(Self::Success),
            0x01 => Ok(Self::Failure),
            0x7e => Ok(Self::NotAuthorized),
            0x7f => Ok(Self::Reserved),
            0x80 => Ok(Self::MalformedCommand),
            0x81 => Ok(Self::UnsupCommand),
            0x85 => Ok(Self::InvalidField),
            0x86 => Ok(Self::UnsupportedAttribute),
            0x87 => Ok(Self::InvalidValue),
            0x88 => Ok(Self::ReadOnly),
            0x89 => Ok(Self::InsufficientSpace),
            0x8b => Ok(Self::NotFound),
            0x8c => Ok(Self::UnreportableAttribute),
            0x8d => Ok(Self::InvalidDataType),
            0x8e => Ok(Self::InvalidSelector),
            0x92 => Ok(Self::ReservedInconsistent),
            0x94 => Ok(Self::Timeout),
            0x95 => Ok(Self::Abort),
            0x96 => Ok(Self::InvalidImage),
            0x97 => Ok(Self::WaitForData),
            0x98 => Ok(Self::NoImageAvailable),
            0x99 => Ok(Self::RequireMoreImage),
            0x9a => Ok(Self::NotificationPending),
            0xc2 => Ok(Self::ReservedCalibration),
            0xc3 => Ok(Self::UnsupportedCluster),
            _ => Err(bad_input!("unknown ZCL status byte")),
        }
    }
}

impl<'a> TryRead<'a, ()> for Status {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let raw: u8 = bytes.read_with(offset, ctx::LE)?;
        // Errata: deprecated wire bytes are substituted for their replacements.
        let status = match raw {
            0x82 | 0x83 | 0x84 => Self::UnsupCommand,
            0x8a | 0xc4 => Self::Success,
            0x8f => Self::NotAuthorized,
            0x90 | 0x91 | 0x93 | 0xc0 | 0xc1 => Self::Failure,
            other => Self::from_byte(other)?,
        };
        Ok((status, *offset))
    }
}

#[allow(deprecated)]
impl TryWrite<()> for Status {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        // Errata: deprecated variants are substituted for their replacements on the
        // wire.
        let raw: u8 = match self {
            Self::Unknown => return Err(bad_input!("unknown ZCL status")),
            Self::UnsupGeneralCommand
            | Self::UnsupManufClusterCommand
            | Self::UnsupManufGeneralCommand => Self::UnsupCommand as u8,
            Self::DuplicateExists | Self::LimitReached => Self::Success as u8,
            Self::WriteOnly => Self::NotAuthorized as u8,
            Self::InconsistentStartupState
            | Self::DefinedOutOfBand
            | Self::ActionDenied
            | Self::HardwareFailure
            | Self::SoftwareFailure => Self::Failure as u8,
            other => other as u8,
        };
        bytes.write_with(offset, raw, ctx::LE)?;
        Ok(*offset)
    }
}

#[derive(Debug, PartialEq)]
pub enum GeneralCommand<'a, const CMD_ATTR_CAPACITY: usize = 16> {
    ReadAttributes(Vec<ReadAttribute, CMD_ATTR_CAPACITY>),
    ReadAttributesResponse(Vec<ReadAttributeResponse<'a>, CMD_ATTR_CAPACITY>),
    WriteAttributes(Vec<WriteAttribute<'a>, CMD_ATTR_CAPACITY>),
    WriteAttributesResponse(Vec<WriteAttributeStatus, CMD_ATTR_CAPACITY>),
    ReportAttributes(Vec<AttributeReport<'a>, CMD_ATTR_CAPACITY>),
    DefaultResponse(DefaultResponse),
}

impl_byte! {
    #[derive(Debug, PartialEq, Eq)]
    pub struct ReadAttribute {
        pub attribute_id: u16,
    }
}

#[derive(Debug, PartialEq)]
pub struct ReadAttributeResponse<'a> {
    pub attribute_id: u16,
    pub status: Status,
    pub value: Option<ZclDataType<'a>>,
}

impl<'a> TryRead<'a, ()> for ReadAttributeResponse<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let attribute_id = bytes.read_with(offset, ctx::LE)?;
        let status: Status = bytes.read_with(offset, ())?;
        let value = if status == Status::Success {
            let data_type: u8 = bytes.read_with(offset, ctx::LE)?;
            Some(bytes.read_with(offset, data_type)?)
        } else {
            None
        };

        Ok((
            Self {
                attribute_id,
                status,
                value,
            },
            *offset,
        ))
    }
}

impl TryWrite<()> for ReadAttributeResponse<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.attribute_id, ctx::LE)?;
        bytes.write_with(offset, self.status, ())?;
        if self.status == Status::Success {
            let value = self.value.ok_or(bad_input!(
                "successful ReadAttributeResponse requires value"
            ))?;
            let type_id = value.type_id()?;
            bytes.write_with(offset, type_id, ctx::LE)?;
            bytes.write_with(offset, value, type_id)?;
        } else if self.value.is_some() {
            return Err(bad_input!(
                "failed ReadAttributeResponse must not include value"
            ));
        }
        Ok(*offset)
    }
}

#[derive(Debug, PartialEq)]
pub struct WriteAttribute<'a> {
    pub attribute_id: u16,
    pub value: ZclDataType<'a>,
}

impl<'a> TryRead<'a, ()> for WriteAttribute<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let attribute_id = bytes.read_with(offset, ctx::LE)?;
        let data_type_id: u8 = bytes.read_with(offset, ctx::LE)?;
        let data_type = bytes.read_with(offset, data_type_id)?;

        Ok((
            Self {
                attribute_id,
                value: data_type,
            },
            *offset,
        ))
    }
}

impl TryWrite<()> for WriteAttribute<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        let data_type_id = self.value.type_id()?;
        bytes.write_with(offset, self.attribute_id, ctx::LE)?;
        bytes.write_with(offset, data_type_id, ctx::LE)?;
        bytes.write_with(offset, self.value, data_type_id)?;
        Ok(*offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteAttributeStatus {
    pub status: Status,
    pub attribute_id: Option<u16>,
}

impl<'a> TryRead<'a, ()> for WriteAttributeStatus {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let status: Status = bytes.read_with(offset, ())?;
        let attribute_id = if status == Status::Success {
            None
        } else {
            Some(bytes.read_with(offset, ctx::LE)?)
        };
        Ok((
            Self {
                status,
                attribute_id,
            },
            *offset,
        ))
    }
}

impl TryWrite<()> for WriteAttributeStatus {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.status, ())?;
        match (self.status, self.attribute_id) {
            (Status::Success, None) => {}
            (Status::Success, Some(_)) => {
                return Err(bad_input!(
                    "successful WriteAttributeStatus must not include attribute id"
                ));
            }
            (_, Some(attribute_id)) => bytes.write_with(offset, attribute_id, ctx::LE)?,
            (_, None) => {
                return Err(bad_input!(
                    "failed WriteAttributeStatus requires attribute id"
                ));
            }
        }
        Ok(*offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultResponse {
    pub command_identifier: u8,
    pub status: Status,
}

impl<'a> TryRead<'a, ()> for DefaultResponse {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let command_identifier = bytes.read_with(offset, ctx::LE)?;
        let status = bytes.read_with(offset, ())?;
        Ok((
            Self {
                command_identifier,
                status,
            },
            *offset,
        ))
    }
}

impl TryWrite<()> for DefaultResponse {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.command_identifier, ctx::LE)?;
        bytes.write_with(offset, self.status, ())?;
        Ok(*offset)
    }
}

#[derive(Debug, PartialEq)]
pub struct AttributeReport<'a> {
    pub attribute_id: u16,
    pub value: ZclDataType<'a>,
}

impl<'a> TryRead<'a, ()> for AttributeReport<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let attribute_id: u16 = bytes.read_with(offset, ctx::LE)?;
        let data_type: u8 = bytes.read_with(offset, ctx::LE)?;
        let data: ZclDataType = bytes.read_with(offset, data_type)?;

        let report = Self {
            attribute_id,
            value: data,
        };

        Ok((report, *offset))
    }
}

impl TryWrite<()> for AttributeReport<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        let data_type_id = self.value.type_id()?;
        bytes.write_with(offset, self.attribute_id, ctx::LE)?;
        bytes.write_with(offset, data_type_id, ctx::LE)?;
        bytes.write_with(offset, self.value, data_type_id)?;
        Ok(*offset)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]
    use byte::TryRead;
    use byte::TryWrite;

    use super::*;
    use crate::common::data_types::EnumN;
    use crate::common::data_types::SignedN;
    use crate::common::data_types::UnsignedN;
    use crate::common::data_types::ZclString;

    #[test]
    fn parse_attribute_report_payload() {
        // given
        let input: &[u8] = &[
            0x00, 0x00, // identifier
            0x29, 0xab, 0x03,
        ];

        // when
        let (report, _) = AttributeReport::try_read(input, ())
            .expect("Failed to read AttributeReport payload in test");

        // then
        assert_eq!(report.attribute_id, 0u16);
        assert_eq!(report.value, ZclDataType::SignedInt(SignedN::Int16(939)));
    }

    #[allow(clippy::panic)]
    #[test]
    fn zcl_general_command() {
        // given
        let input: &[u8] = &[
            0x18, // frame control
            0x01, // sequence number
            0x0A, // command identifier
            0x00, 0x00, 0x29, 0x3f, 0x0a, // payload
        ];

        // when
        let (frame, _) = ZclFrame::try_read(input, ()).expect("Failed to read ZclFrame");

        // then
        assert!(matches!(frame.payload, ZclFramePayload::GeneralCommand(_)));

        if let ZclFramePayload::GeneralCommand(cmd) = frame.payload {
            if let GeneralCommand::ReportAttributes(report) = cmd {
                assert_eq!(report.len(), 1);
                let attribute_report = report.first().expect("Expected ONE report in test");
                assert_eq!(attribute_report.attribute_id, 0u16);
                assert_eq!(
                    attribute_report.value,
                    ZclDataType::SignedInt(SignedN::Int16(2623))
                );
            } else {
                panic!("Report Attributes Command expected!");
            }
        } else {
            panic!("GeneralCommand expected!");
        }
    }

    #[allow(clippy::panic)]
    #[test]
    fn cluster_specific_command() {
        // given
        let input: &[u8] = &[
            0x19, // frame control
            0x01, // sequence number
            0x01, // command identifier
            0x00, 0x00, 0x29, 0x3f, 0x0a, // payload
        ];

        // when
        let (frame, _) = ZclFrame::try_read(input, ()).expect("Failed to read ZclFrame");

        // then
        let expected = &[0x00, 0x00, 0x29, 0x3f, 0x0a];
        assert!(matches!(
            frame.payload,
            ZclFramePayload::ClusterSpecificCommand(_)
        ));
        if let ZclFramePayload::ClusterSpecificCommand(cmd) = frame.payload {
            assert_eq!(cmd, expected);
        } else {
            panic!("ClusterSpecificCommand expected!");
        }
    }

    #[test]
    fn read_attributes_frame_roundtrips() {
        let input: &[u8] = &[
            0x00, // frame control: global, client to server
            0x11, // sequence number
            0x00, // Read Attributes
            0x00, 0x00, // ZCLVersion
            0x04, 0x00, // ManufacturerName
        ];

        let (frame, len) = ZclFrame::try_read(input, ()).expect("read attributes request parses");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributes(attrs)) = &frame.payload
        else {
            panic!("ReadAttributesCommand expected");
        };
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].attribute_id, 0x0000);
        assert_eq!(attrs[1].attribute_id, 0x0004);

        let mut output = [0u8; 16];
        let written = frame
            .try_write(&mut output, ())
            .expect("read attributes request writes");
        assert_eq!(written, input.len());
        assert_eq!(&output[..written], input);
    }

    #[test]
    fn write_attributes_frame_roundtrips() {
        let input: &[u8] = &[
            0x00, // frame control: global, client to server
            0x12, // sequence number
            0x02, // Write Attributes
            0x01, 0x00, // attribute id
            0x10, // boolean
            0x01, // true
            0x02, 0x00, // attribute id
            0x42, // character string
            0x02, b'O', b'K',
        ];

        let (frame, len) = ZclFrame::try_read(input, ()).expect("write attributes request parses");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributes(attrs)) =
            &frame.payload
        else {
            panic!("WriteAttributesCommand expected");
        };
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].value, ZclDataType::Bool(true));
        assert_eq!(
            attrs[1].value,
            ZclDataType::String(ZclString::CharString("OK"))
        );

        let mut output = [0u8; 32];
        let written = frame
            .try_write(&mut output, ())
            .expect("write attributes request writes");
        assert_eq!(written, input.len());
        assert_eq!(&output[..written], input);
    }

    #[test]
    fn basic_cluster_read_attributes_response_writes_spec_records() {
        let mut records = Vec::<ReadAttributeResponse<'_>, 16>::new();
        records
            .push(ReadAttributeResponse {
                attribute_id: 0x0000, // ZCLVersion
                status: Status::Success,
                value: Some(ZclDataType::UnsignedInt(UnsignedN::Uint8(8))),
            })
            .unwrap();
        records
            .push(ReadAttributeResponse {
                attribute_id: 0x0007, // PowerSource
                status: Status::Success,
                value: Some(ZclDataType::Enum(EnumN::Enum8(0x03))),
            })
            .unwrap();
        records
            .push(ReadAttributeResponse {
                attribute_id: 0x0004, // ManufacturerName
                status: Status::Success,
                value: Some(ZclDataType::String(ZclString::CharString("Acme"))),
            })
            .unwrap();

        let frame = ZclFrame {
            header: ZclHeader {
                frame_control: crate::header::frame_control::FrameControl(0x18),
                manufacturer_code: None,
                sequence_number: 0x22,
                command_identifier:
                    crate::header::command_identifier::CommandIdentifier::ReadAttributesResponse,
            },
            payload: ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributesResponse(
                records,
            )),
        };

        let expected: &[u8] = &[
            0x18, 0x22, 0x01, // header
            0x00, 0x00, 0x00, 0x20, 0x08, // ZCLVersion: success, uint8, 8
            0x07, 0x00, 0x00, 0x30, 0x03, // PowerSource: success, enum8, battery
            0x04, 0x00, 0x00, 0x42, 0x04, b'A', b'c', b'm', b'e',
        ];

        let mut output = [0u8; 32];
        let written = frame
            .try_write(&mut output, ())
            .expect("basic cluster read response writes");
        assert_eq!(written, expected.len());
        assert_eq!(&output[..written], expected);

        let (parsed, parsed_len) =
            ZclFrame::try_read(&output[..written], ()).expect("basic cluster read response parses");
        assert_eq!(parsed_len, expected.len());
        assert_eq!(parsed.header.sequence_number, 0x22);
    }

    #[test]
    fn write_attributes_response_all_success_roundtrips() {
        // All writes succeeded: single SUCCESS record, no attribute id.
        let input: &[u8] = &[
            0x18, // frame control: global, server to client, default response disabled
            0x13, // sequence number
            0x04, // Write Attributes Response
            0x00, // SUCCESS — attribute id omitted
        ];

        let (frame, len) =
            ZclFrame::try_read(input, ()).expect("all-success write response parses");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributesResponse(ref records)) =
            frame.payload
        else {
            panic!("WriteAttributesResponse expected");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, Status::Success);
        assert!(records[0].attribute_id.is_none());

        let mut output = [0u8; 8];
        let written = frame
            .try_write(&mut output, ())
            .expect("all-success write response writes");
        assert_eq!(written, input.len());
        assert_eq!(&output[..written], input);
    }

    #[test]
    fn write_attributes_response_failure_records_roundtrip() {
        // Some writes failed: only the failed records are present, each with attribute
        // id.
        let input: &[u8] = &[
            0x18, // frame control: global, server to client, default response disabled
            0x14, // sequence number
            0x04, // Write Attributes Response
            0x86, // UNSUPPORTED_ATTRIBUTE
            0x99, 0x88, // attribute id
        ];

        let (frame, len) = ZclFrame::try_read(input, ()).expect("failure write response parses");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributesResponse(ref records)) =
            frame.payload
        else {
            panic!("WriteAttributesResponse expected");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, Status::UnsupportedAttribute);
        assert_eq!(records[0].attribute_id, Some(0x8899));

        let mut output = [0u8; 16];
        let written = frame
            .try_write(&mut output, ())
            .expect("failure write response writes");
        assert_eq!(written, input.len());
        assert_eq!(&output[..written], input);
    }

    #[test]
    fn write_attributes_response_parses_mixed_success_and_failure_gracefully() {
        // Non-compliant senders may mix SUCCESS with failures; parse gracefully.
        let input: &[u8] = &[
            0x18, // frame control
            0x15, // sequence number
            0x04, // Write Attributes Response
            0x00, // SUCCESS record — no attr id
            0x86, // UNSUPPORTED_ATTRIBUTE record
            0x01, 0x00, // attribute id
        ];
        let (frame, len) = ZclFrame::try_read(input, ()).expect("mixed response parses gracefully");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributesResponse(ref records)) =
            frame.payload
        else {
            panic!("WriteAttributesResponse expected");
        };
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].status, Status::Success);
        assert!(records[0].attribute_id.is_none());
        assert_eq!(records[1].status, Status::UnsupportedAttribute);
        assert_eq!(records[1].attribute_id, Some(0x0001));
    }

    #[test]
    fn write_attributes_response_normalizes_mixed_to_failures_on_write() {
        // TryWrite drops SUCCESS records when failures are present.
        let mut records = Vec::<WriteAttributeStatus, 16>::new();
        records
            .push(WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            })
            .unwrap();
        records
            .push(WriteAttributeStatus {
                status: Status::UnsupportedAttribute,
                attribute_id: Some(0x0001),
            })
            .unwrap();

        let frame = ZclFrame {
            header: ZclHeader {
                frame_control: crate::header::frame_control::FrameControl(0x18),
                manufacturer_code: None,
                sequence_number: 0x20,
                command_identifier:
                    crate::header::command_identifier::CommandIdentifier::WriteAttributesResponse,
            },
            payload: ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributesResponse(
                records,
            )),
        };

        let mut buf = [0u8; 16];
        let written = frame
            .try_write(&mut buf, ())
            .expect("normalizes mixed on write");
        let expected: &[u8] = &[
            0x18, 0x20, 0x04, // header
            0x86, 0x01, 0x00, // UNSUPPORTED_ATTRIBUTE for attribute 0x0001
        ];
        assert_eq!(&buf[..written], expected);
    }

    #[test]
    fn write_attributes_response_normalizes_multiple_success_to_single() {
        // Multiple SUCCESS records collapse to a single SUCCESS on write.
        let mut records = Vec::<WriteAttributeStatus, 16>::new();
        records
            .push(WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            })
            .unwrap();
        records
            .push(WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            })
            .unwrap();

        let frame = ZclFrame {
            header: ZclHeader {
                frame_control: crate::header::frame_control::FrameControl(0x18),
                manufacturer_code: None,
                sequence_number: 0x20,
                command_identifier:
                    crate::header::command_identifier::CommandIdentifier::WriteAttributesResponse,
            },
            payload: ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributesResponse(
                records,
            )),
        };

        let mut buf = [0u8; 8];
        let written = frame
            .try_write(&mut buf, ())
            .expect("normalizes multiple success");
        let expected: &[u8] = &[
            0x18, 0x20, 0x04, // header
            0x00, // single SUCCESS
        ];
        assert_eq!(&buf[..written], expected);
    }

    #[test]
    fn unsupported_attribute_data_type_is_rejected() {
        let input: &[u8] = &[
            0x18, // frame control: global, server to client
            0x15, // sequence number
            0x01, // Read Attributes Response
            0x00, 0x00, // attribute id
            0x00, // success
            0x48, // array: unsupported until structured types are implemented
        ];

        assert!(ZclFrame::try_read(input, ()).is_err());
    }

    #[test]
    fn default_response_roundtrips() {
        let input: &[u8] = &[
            0x18, // frame control: global, server to client, default response disabled
            0x14, // sequence number
            0x0b, // Default Response
            0x00, // command identifier being responded to: Read Attributes
            0x00, // success
        ];

        let (frame, len) = ZclFrame::try_read(input, ()).expect("default response parses");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::DefaultResponse(ref dr)) = frame.payload
        else {
            panic!("DefaultResponse expected");
        };
        assert_eq!(dr.command_identifier, 0x00); // Read Attributes
        assert_eq!(dr.status, Status::Success);

        let mut output = [0u8; 8];
        let written = frame
            .try_write(&mut output, ())
            .expect("default response writes");
        assert_eq!(written, input.len());
        assert_eq!(&output[..written], input);
    }

    #[test]
    fn default_response_rejects_unrecognized_status_byte() {
        let input: &[u8] = &[
            0x18, // frame control
            0x14, // sequence number
            0x0b, // Default Response
            0x02, // command identifier: Write Attributes
            0x7d, // unrecognized status byte
        ];

        assert!(ZclFrame::try_read(input, ()).is_err());
    }

    #[test]
    fn read_attribute_response_failure_record_roundtrips() {
        let input: &[u8] = &[
            0x18, // frame control: global, server to client
            0x16, // sequence number
            0x01, // Read Attributes Response
            0x00, 0x00, // attribute id: ZCLVersion
            0x86, // UNSUPPORTED_ATTRIBUTE — no data type or value follows
        ];

        let (frame, len) = ZclFrame::try_read(input, ()).expect("failure record parses");
        assert_eq!(len, input.len());

        let ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributesResponse(ref records)) =
            frame.payload
        else {
            panic!("ReadAttributesResponse expected");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].attribute_id, 0x0000);
        assert_eq!(records[0].status, Status::UnsupportedAttribute);
        assert!(records[0].value.is_none());

        let mut output = [0u8; 16];
        let written = frame
            .try_write(&mut output, ())
            .expect("failure record writes");
        assert_eq!(written, input.len());
        assert_eq!(&output[..written], input);
    }

    #[test]
    fn read_attribute_response_rejects_inconsistent_status_value_combinations() {
        let mut buf = [0u8; 16];

        // success status requires a value on write
        let no_value = ReadAttributeResponse {
            attribute_id: 0x0000,
            status: Status::Success,
            value: None,
        };
        assert!(no_value.try_write(&mut buf, ()).is_err());

        // failure status must not carry a value on write
        let spurious_value = ReadAttributeResponse {
            attribute_id: 0x0000,
            status: Status::UnsupportedAttribute,
            value: Some(ZclDataType::UnsignedInt(UnsignedN::Uint8(0))),
        };
        assert!(spurious_value.try_write(&mut buf, ()).is_err());
    }
}
