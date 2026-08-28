//! Generic attribute access.
//!
//! See ZCL Section 2.5.1 / 2.5.3.
//!
//! A cluster server exposes its attributes by implementing [`AttributeSource`];
//! [`handle_read_attributes`] then answers `Read Attributes` for it, so every
//! cluster gets attribute reads without repeating the frame handling. A server
//! with writable attributes additionally implements [`AttributeSink`] and is
//! answered by [`handle_write_attributes`].

use byte::BytesExt;
use byte::TryRead;
use heapless::Vec;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::common::data_types::ZclDataType;
use crate::frame::DefaultResponse;
use crate::frame::GeneralCommand;
use crate::frame::ReadAttributeResponse;
use crate::frame::Status;
use crate::frame::WriteAttributeStatus;
use crate::frame::ZclFrame;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameControl;
use crate::payload::ZclFramePayload;

/// Frame control for a global, server→client response with the default
/// response disabled.
pub(crate) const RESPONSE_FRAME_CONTROL: u8 = 0x18;

// attribute records carried in one response frame
const MAX_RECORDS: usize = 16;

/// A cluster server that can resolve attribute identifiers to values.
pub trait AttributeSource {
    /// Cluster this source answers for.
    fn cluster_id(&self) -> u16;

    /// Current value of `attribute_id`, or `None` when unsupported.
    fn attribute(&self, attribute_id: u16) -> Option<ZclDataType<'_>>;
}

/// A cluster server whose attributes can be written (ZCL 2.5.3).
pub trait AttributeSink: AttributeSource {
    /// Apply a write, returning the ZCL status for the record: `Success`,
    /// `UnsupportedAttribute`, `ReadOnly`, `InvalidDataType` or `InvalidValue`.
    fn set_attribute(&self, attribute_id: u16, value: &ZclDataType<'_>) -> Status;
}

/// Answer a `Read Attributes` request (ZCL 2.5.1) from `source`.
///
/// Returns `None` for another cluster, another command, or an unparsable
/// frame, leaving the caller free to try another handler. An unsupported
/// attribute is reported per-record rather than failing the request.
pub fn handle_read_attributes<S>(
    source: &S,
    cluster_id: u16,
    asdu: &[u8],
    out: &mut [u8],
) -> Option<usize>
where
    S: AttributeSource + ?Sized,
{
    if cluster_id != source.cluster_id() {
        return None;
    }

    let (frame, _) = ZclFrame::try_read(asdu, ()).ok()?;
    let ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributes(requests)) = frame.payload
    else {
        return None;
    };

    // more records than fit are dropped: a partial response beats none
    // (ZCL 2.5.1.3)
    let mut records: Vec<ReadAttributeResponse<'_>, MAX_RECORDS> = Vec::new();
    for request in requests {
        let record = source.attribute(request.attribute_id).map_or(
            ReadAttributeResponse {
                attribute_id: request.attribute_id,
                status: Status::UnsupportedAttribute,
                value: None,
            },
            |value| ReadAttributeResponse {
                attribute_id: request.attribute_id,
                status: Status::Success,
                value: Some(value),
            },
        );
        if records.push(record).is_err() {
            break;
        }
    }

    write_response(
        frame.header.sequence_number,
        CommandIdentifier::ReadAttributesResponse,
        GeneralCommand::ReadAttributesResponse(records),
        out,
    )
}

/// Answer a `Write Attributes` request (ZCL 2.5.3) against `sink`.
///
/// Returns `None` for another cluster, another command, or an unparsable
/// frame. Every record is attempted; the response carries a single `Success`
/// record when they all succeeded (ZCL 2.5.4.1.2).
pub fn handle_write_attributes<S>(
    sink: &S,
    cluster_id: u16,
    asdu: &[u8],
    out: &mut [u8],
) -> Option<usize>
where
    S: AttributeSink + ?Sized,
{
    if cluster_id != sink.cluster_id() {
        return None;
    }

    let (frame, _) = ZclFrame::try_read(asdu, ()).ok()?;
    let ZclFramePayload::GeneralCommand(GeneralCommand::WriteAttributes(requests)) = frame.payload
    else {
        return None;
    };

    let mut records: Vec<WriteAttributeStatus, MAX_RECORDS> = Vec::new();
    for request in &requests {
        let status = sink.set_attribute(request.attribute_id, &request.value);
        if status == Status::Success {
            continue;
        }
        let record = WriteAttributeStatus {
            status,
            attribute_id: Some(request.attribute_id),
        };
        if records.push(record).is_err() {
            break;
        }
    }

    if records.is_empty() {
        records
            .push(WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            })
            .ok()?;
    }

    write_response(
        frame.header.sequence_number,
        CommandIdentifier::WriteAttributesResponse,
        GeneralCommand::WriteAttributesResponse(records),
        out,
    )
}

/// Build a Default Response (ZCL 2.5.12) for the command `header` describes.
///
/// Returns `None` when the spec forbids one (2.5.12.2): the request was not
/// unicast, or it disabled the default response. `status` reports how the
/// command was handled — `UnsupCommand` for an unrecognized command id.
pub fn default_response(
    header: &ZclHeader,
    unicast: bool,
    status: Status,
    out: &mut [u8],
) -> Option<usize> {
    if !unicast || header.frame_control.disable_default_response() {
        return None;
    }

    write_response(
        header.sequence_number,
        CommandIdentifier::DefaultResponse,
        GeneralCommand::DefaultResponse(DefaultResponse {
            command_identifier: header.command_identifier.raw(),
            status,
        }),
        out,
    )
}

fn write_response(
    sequence_number: u8,
    command_identifier: CommandIdentifier,
    payload: GeneralCommand<'_>,
    out: &mut [u8],
) -> Option<usize> {
    let response = ZclFrame {
        header: ZclHeader {
            frame_control: FrameControl(RESPONSE_FRAME_CONTROL),
            manufacturer_code: None,
            sequence_number,
            command_identifier,
        },
        payload: ZclFramePayload::GeneralCommand(payload),
    };

    let offset = &mut 0;
    out.write_with(offset, response, ()).ok()?;
    Some(*offset)
}

/// Adapts any [`AttributeSource`] into a read-only cluster server.
///
/// A cluster with its own commands implements [`ClusterRequestHandler`]
/// directly and calls [`handle_read_attributes`] for the attribute half.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttributeServer<S>(pub S);

impl<S: AttributeSource> ClusterRequestHandler for AttributeServer<S> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<usize> {
        handle_read_attributes(&self.0, request.cluster_id, request.asdu, out)
    }
}
