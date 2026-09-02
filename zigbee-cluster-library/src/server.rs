//! Cluster servers and generic attribute access.
//!
//! See ZCL Section 2.5.1 / 2.5.3 / 2.5.12.
//!
//! A cluster is served by implementing [`ClusterServer`] and delegating
//! [`ClusterRequestHandler`] to [`handle_cluster_request`], which parses the
//! ZCL header once and routes the frame: `Read Attributes` and
//! `Write Attributes` are answered from the server's attribute methods, and a
//! cluster-specific command is passed to [`ClusterServer::command`].
//!
//! Attributes are reached through their descriptors rather than a dynamic
//! value: a server encodes with
//! [`Attribute::encode`](crate::types::descriptors::Attribute::encode) and
//! decodes with
//! [`Attribute::decode`](crate::types::descriptors::Attribute::decode), so an
//! attribute can only be answered with the type the specification gives it.

use byte::BytesExt;
use byte::ctx;
use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;

use crate::frame::Status;
use crate::frame::header::ZclHeader;
use crate::frame::header::command_identifier::CommandIdentifier;
use crate::frame::header::frame_control::FrameControl;
use crate::frame::header::frame_control::FrameType;
use crate::types::data_types::ZclDataType;
use crate::types::descriptors::Cluster;
use crate::types::ids::AttributeId;
use crate::types::ids::CommandId;
use crate::types::ids::TypeId;

/// Frame control for a global, server→client response with the default
/// response disabled.
pub(crate) const RESPONSE_FRAME_CONTROL: u8 = 0x18;

/// A cluster-specific command addressed to a [`ClusterServer`].
#[derive(Debug, Clone, Copy)]
pub struct ClusterCommand<'a> {
    /// Command identifier, in the cluster's own numbering (2.4.1.4).
    pub id: CommandId,
    /// Command payload, the bytes following the ZCL header.
    pub data: &'a [u8],
    /// Sequence number to echo in a response (2.4.1.3).
    pub sequence_number: u8,
}

/// How a [`ClusterServer`] handled a cluster-specific command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    /// A complete response frame was written, of this many bytes. A command
    /// answered by its own cluster response draws no Default Response
    /// (2.5.12.2).
    Response(usize),
    /// Nothing was written; the dispatcher emits a Default Response carrying
    /// this status.
    Status(Status),
}

/// A server for one cluster.
///
/// [`cluster`](Self::cluster) and [`encode_value`](Self::encode_value) are all
/// a read-only cluster without commands has to provide. Writable attributes
/// override [`decode_value`](Self::decode_value); a cluster with its own
/// commands overrides [`command`](Self::command).
pub trait ClusterServer {
    /// Cluster this server answers for.
    fn cluster(&self) -> Cluster;

    /// Append the `type | value` pair for `id` to `out`.
    ///
    /// Returns `Success` when the attribute was written, or the status that
    /// explains why it was not: `UnsupportedAttribute` for an attribute this
    /// server does not have, `InsufficientSpace` when it does not fit.
    /// Implementations write through
    /// [`Attribute::encode`](crate::types::descriptors::Attribute::encode) so
    /// the value carries the declared type.
    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status;

    /// Apply a write, consuming the value bytes from `bytes` at `offset`
    /// (2.5.3).
    ///
    /// Implementations decode through
    /// [`Attribute::decode`](crate::types::descriptors::Attribute::decode),
    /// which rejects a type identifier the attribute was not declared with,
    /// and advance `offset` only when the record was decoded; the dispatcher
    /// resynchronizes otherwise.
    ///
    /// The default reports every attribute as read-only, which is what a
    /// cluster with no writable attributes owes the requester (2.5.4.1.2).
    /// Override to distinguish an attribute this server does not have, which
    /// is answered `UnsupportedAttribute`.
    fn decode_value(
        &self,
        _id: AttributeId,
        _type_id: TypeId,
        _bytes: &[u8],
        _offset: &mut usize,
    ) -> Status {
        Status::ReadOnly
    }

    /// Handle a cluster-specific command (2.4.1.1.1).
    ///
    /// The default reports every command as unsupported, which is what a
    /// cluster with only global commands owes the requester.
    fn command(&self, _command: ClusterCommand<'_>, _out: &mut [u8]) -> CommandOutcome {
        CommandOutcome::Status(Status::UnsupCommand)
    }

    /// Route a request to this server, parsing the ZCL header once: `Read
    /// Attributes` and `Write Attributes` are answered from the attribute
    /// methods above, a cluster-specific command goes to
    /// [`command`](Self::command), and anything else draws a Default Response.
    ///
    /// Returns `None` for another cluster, an unparsable frame, or a command
    /// that draws no response, leaving the caller free to try another handler.
    /// Implement `ClusterRequestHandler` by delegating to this; a blanket
    /// impl is not possible because that trait belongs to `zigbee-core`.
    ///
    /// A ZCL response travels back on the request's own cluster and profile
    /// with the endpoints swapped (2.4.1), which is applied here rather than
    /// by each cluster.
    fn handle_request(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        if request.cluster_id != self.cluster().id().0 {
            return None;
        }

        // the header is parsed once here; the handlers below take the body
        let body_at = &mut 0;
        let header: ZclHeader = request.asdu.read_with(body_at, ()).ok()?;
        let body = request.asdu.get(*body_at..)?;

        let len = match header.frame_control.frame_type() {
            FrameType::GlobalCommand => match header.command_identifier {
                CommandIdentifier::ReadAttributes => read_attributes(self, header, body, out),
                CommandIdentifier::WriteAttributes => write_attributes(self, header, body, out),
                _ => default_response(header, request.unicast, Status::UnsupCommand, out),
            },
            FrameType::ClusterCommand => {
                let command = ClusterCommand {
                    id: CommandId(header.command_identifier.raw()),
                    data: body,
                    sequence_number: header.sequence_number,
                };
                match self.command(command, out) {
                    CommandOutcome::Response(len) => Some(len),
                    CommandOutcome::Status(status) => {
                        default_response(header, request.unicast, status, out)
                    }
                }
            }
            FrameType::Reserved => None,
        }?;

        Some(ClusterReply::matching(request, len))
    }
}

/// Answer `Read Attributes` (2.5.1).
///
/// An unsupported attribute is reported per-record rather than failing the
/// request; records that no longer fit are dropped, since a partial response
/// beats none (2.5.1.3).
fn read_attributes<S>(server: &S, header: ZclHeader, body: &[u8], out: &mut [u8]) -> Option<usize>
where
    S: ClusterServer + ?Sized,
{
    let offset = &mut 0;
    write_response_header(
        header.sequence_number,
        CommandIdentifier::ReadAttributesResponse,
        out,
        offset,
    )?;

    // the request body is a bare list of attribute identifiers (2.5.1.1)
    let request = &mut 0;
    while *request < body.len() {
        let id: u16 = body.read_with(request, ctx::LE).ok()?;

        let record = *offset;
        if write_read_record(server, AttributeId(id), out, offset).is_none() {
            // this record did not fit; return the frame without it
            *offset = record;
            break;
        }
    }

    Some(*offset)
}

/// Write one `Read Attributes Response` record (2.5.2.1), rolling back to a
/// status-only record when the value could not be encoded.
fn write_read_record<S>(
    server: &S,
    id: AttributeId,
    out: &mut [u8],
    offset: &mut usize,
) -> Option<()>
where
    S: ClusterServer + ?Sized,
{
    out.write_with(offset, id.0, ctx::LE).ok()?;

    // the status is only known once the value has been attempted, so reserve
    // its byte and patch it afterwards
    let status_at = *offset;
    out.write_with(offset, Status::Success, ()).ok()?;

    let value_at = *offset;
    let status = server.encode_value(id, out, offset);
    if status != Status::Success {
        // a failed record carries the status alone (2.5.2.1.2)
        *offset = value_at;
        let patch = &mut { status_at };
        out.write_with(patch, status, ()).ok()?;
    }

    Some(())
}

/// Answer `Write Attributes` (2.5.3).
///
/// Every record is attempted; the response carries a single `Success` record
/// when they all succeeded (2.5.4.1.2).
fn write_attributes<S>(server: &S, header: ZclHeader, body: &[u8], out: &mut [u8]) -> Option<usize>
where
    S: ClusterServer + ?Sized,
{
    let offset = &mut 0;
    write_response_header(
        header.sequence_number,
        CommandIdentifier::WriteAttributesResponse,
        out,
        offset,
    )?;

    let request = &mut 0;
    let mut failures = 0usize;
    while *request < body.len() {
        let id: u16 = body.read_with(request, ctx::LE).ok()?;
        let raw_type: u8 = body.read_with(request, ctx::LE).ok()?;

        let value_at = *request;
        let status = server.decode_value(AttributeId(id), TypeId::from_u8(raw_type), body, request);
        if status == Status::Success {
            continue;
        }

        // the server left the offset untouched, so step over the value with
        // the length its own type byte implies to stay aligned on the next
        // record
        *request = value_at;
        let _: ZclDataType<'_> = body.read_with(request, raw_type).ok()?;

        failures += 1;
        out.write_with(offset, status, ()).ok()?;
        out.write_with(offset, id, ctx::LE).ok()?;
    }

    // all-success is reported as one status record with no attribute id
    if failures == 0 {
        out.write_with(offset, Status::Success, ()).ok()?;
    }

    Some(*offset)
}

/// Build a Default Response (2.5.12) for the command `header` describes.
///
/// Returns `None` when the spec forbids one (2.5.12.2): the request was not
/// unicast, or it disabled the default response. `status` reports how the
/// command was handled — `UnsupCommand` for an unrecognized command id.
fn default_response(
    header: ZclHeader,
    unicast: bool,
    status: Status,
    out: &mut [u8],
) -> Option<usize> {
    if !unicast || header.frame_control.disable_default_response() {
        return None;
    }

    let offset = &mut 0;
    write_response_header(
        header.sequence_number,
        CommandIdentifier::DefaultResponse,
        out,
        offset,
    )?;
    out.write_with(offset, header.command_identifier.raw(), ctx::LE)
        .ok()?;
    out.write_with(offset, status, ()).ok()?;
    Some(*offset)
}

fn write_response_header(
    sequence_number: u8,
    command_identifier: CommandIdentifier,
    out: &mut [u8],
    offset: &mut usize,
) -> Option<()> {
    let header = ZclHeader {
        frame_control: FrameControl(RESPONSE_FRAME_CONTROL),
        manufacturer_code: None,
        sequence_number,
        command_identifier,
    };
    out.write_with(offset, header, ()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clusters::general::basic::BasicServer;
    use crate::clusters::general::identify::IdentifyServer;

    const SERVER: BasicServer = BasicServer {
        zcl_version: 8,
        application_version: 1,
        stack_version: 0,
        hw_version: 1,
        manufacturer_name: "zigbee-rs",
        model_identifier: "temp-1",
        power_source: 0x03,
    };

    fn request<'a>(cluster_id: u16, asdu: &'a [u8]) -> ClusterRequest<'a> {
        ClusterRequest {
            profile_id: 0x0104,
            cluster_id,
            src_endpoint: 7,
            dst_endpoint: 1,
            unicast: true,
            asdu,
        }
    }

    // 2.4.1: a ZCL response goes back on the request's own cluster and profile
    // with the endpoints swapped, which the cluster layer decides rather than
    // the APS layer sending it
    #[test]
    fn the_reply_is_addressed_back_to_the_requester() {
        let asdu = [0x00, 0x2a, 0x00, 0x00, 0x00];
        let mut out = [0u8; 32];

        let request = request(0x0000, &asdu);
        let reply = SERVER.handle_request(&request, &mut out).expect("handled");

        assert_eq!(reply.cluster_id, request.cluster_id);
        assert_eq!(reply.profile_id, request.profile_id);
        // the requester's source endpoint is where the answer is delivered
        assert_eq!(reply.dst_endpoint, request.src_endpoint);
        assert_eq!(reply.src_endpoint, request.dst_endpoint);
        assert_eq!(reply.len, 8);
    }

    // 2.5.2.1.2: a record that failed carries the status alone, with neither
    // type nor value, and the records after it still line up
    #[test]
    fn an_unsupported_attribute_does_not_desynchronize_the_response() {
        let asdu = [
            0x00, 0x2a, 0x00, // frame control, sequence, ReadAttributes
            0x99, 0x99, // unsupported
            0x00, 0x00, // ZCLVersion, uint8 8
            0x07, 0x00, // PowerSource, enum8 3
        ];

        let mut out = [0u8; 64];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(
            &out[..reply.len],
            &[
                0x18, 0x2a, 0x01, // frame control, sequence, ReadAttributesResponse
                0x99, 0x99, 0x86, // unsupported attribute, status only
                0x00, 0x00, 0x00, 0x20, 0x08, // ZCLVersion, success, uint8, 8
                0x07, 0x00, 0x00, 0x30, 0x03, // PowerSource, success, enum8, 3
            ]
        );
    }

    // the descriptor gives ManufacturerName type 0x42, so that is what the
    // response carries without the server restating it
    #[test]
    fn the_response_type_byte_comes_from_the_descriptor() {
        let asdu = [0x00, 0x01, 0x00, 0x04, 0x00];
        let mut out = [0u8; 64];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(&out[3..5], &[0x04, 0x00]);
        assert_eq!(out[5], Status::Success as u8);
        assert_eq!(out[6], TypeId::CharacterString.as_u8());
        assert_eq!(&out[7..reply.len], b"\x09zigbee-rs");
    }

    // 2.5.1.3: a response that no longer fits is truncated rather than dropped
    #[test]
    fn records_that_do_not_fit_are_dropped() {
        let asdu = [
            0x00, 0x01, 0x00, // frame control, sequence, ReadAttributes
            0x00, 0x00, // ZCLVersion
            0x04, 0x00, // ManufacturerName, will not fit
        ];

        let mut out = [0u8; 9];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(
            &out[..reply.len],
            &[0x18, 0x01, 0x01, 0x00, 0x00, 0x00, 0x20, 0x08]
        );
    }

    // 2.5.3: a record whose type byte disagrees with the attribute is rejected,
    // and the record after it is still applied
    #[test]
    fn a_mistyped_write_is_rejected_and_the_next_record_still_applies() {
        let server = IdentifyServer::new();
        let asdu = [
            0x00, 0x07, 0x02, // frame control, sequence, WriteAttributes
            0x00, 0x00, 0x20, 0x05, // IdentifyTime as uint8 — wrong type
            0x00, 0x00, 0x21, 0x2c, 0x01, // IdentifyTime as uint16, 300
        ];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(0x0003, &asdu), &mut out)
            .expect("handled");

        assert_eq!(server.identify_time(), 300);
        assert_eq!(&out[..reply.len], &[0x18, 0x07, 0x04, 0x8d, 0x00, 0x00]);
    }

    #[test]
    fn a_fully_successful_write_reports_one_success_record() {
        let server = IdentifyServer::new();
        let asdu = [0x00, 0x07, 0x02, 0x00, 0x00, 0x21, 0x0a, 0x00];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(0x0003, &asdu), &mut out)
            .expect("handled");

        assert_eq!(server.identify_time(), 10);
        assert_eq!(&out[..reply.len], &[0x18, 0x07, 0x04, 0x00]);
    }

    // the default `decode_value` answers a write to a cluster with no writable
    // attributes, which previously drew no response at all
    #[test]
    fn a_read_only_cluster_answers_writes_as_read_only() {
        let asdu = [0x00, 0x09, 0x02, 0x00, 0x00, 0x20, 0x05];

        let mut out = [0u8; 32];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        // WriteAttributesResponse carrying ReadOnly for ZCLVersion
        assert_eq!(&out[..reply.len], &[0x18, 0x09, 0x04, 0x88, 0x00, 0x00]);
    }

    // an unrecognized global command draws a Default Response (2.5.12.2)
    #[test]
    fn an_unsupported_global_command_draws_a_default_response() {
        // DiscoverAttributes (0x0c) is not implemented
        let asdu = [0x00, 0x0b, 0x0c, 0x00, 0x00, 0x05];

        let mut out = [0u8; 32];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(&out[..reply.len], &[0x18, 0x0b, 0x0b, 0x0c, 0x81]);
    }
}
