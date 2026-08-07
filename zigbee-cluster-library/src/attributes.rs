//! Generic attribute access.
//!
//! See ZCL Section 2.5.1.
//!
//! A cluster server exposes its attributes by implementing [`AttributeSource`];
//! [`handle_read_attributes`] then answers `Read Attributes` for it, so every
//! cluster gets attribute reads without repeating the frame handling.

use byte::BytesExt;
use byte::TryRead;
use heapless::Vec;
use zigbee::zdo::ClusterRequestHandler;

use crate::common::data_types::ZclDataType;
use crate::frame::GeneralCommand;
use crate::frame::ReadAttributeResponse;
use crate::frame::Status;
use crate::frame::ZclFrame;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameControl;
use crate::payload::ZclFramePayload;

/// Frame control for a global, server→client response with the default
/// response disabled.
pub(crate) const RESPONSE_FRAME_CONTROL: u8 = 0x18;

// attribute records carried in one Read Attributes Response
const MAX_RECORDS: usize = 16;

/// A cluster server that can resolve attribute identifiers to values.
pub trait AttributeSource {
    /// Cluster this source answers for.
    fn cluster_id(&self) -> u16;

    /// Current value of `attribute_id`, or `None` when unsupported.
    fn attribute(&self, attribute_id: u16) -> Option<ZclDataType<'_>>;
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
        records.push(record).ok()?;
    }

    let response = ZclFrame {
        header: ZclHeader {
            frame_control: FrameControl(RESPONSE_FRAME_CONTROL),
            manufacturer_code: None,
            sequence_number: frame.header.sequence_number,
            command_identifier: CommandIdentifier::ReadAttributesResponse,
        },
        payload: ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributesResponse(records)),
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
    fn handle(
        &self,
        _profile_id: u16,
        cluster_id: u16,
        _src_endpoint: u8,
        _dst_endpoint: u8,
        asdu: &[u8],
        out: &mut [u8],
    ) -> Option<usize> {
        handle_read_attributes(&self.0, cluster_id, asdu, out)
    }
}
