//! Cluster servers and generic attribute access.
//!
//! See ZCL Section 2.5.
//!
//! A cluster is served by implementing [`ClusterServer`] and delegating
//! [`ClusterRequestHandler`](zigbee_core::zdo::ClusterRequestHandler) to
//! [`ClusterServer::handle_request`], which parses the ZCL header once and
//! routes the frame: the attribute global commands are answered from the
//! server's attribute methods and its [`attribute list`](ClusterServer::attributes),
//! and a cluster-specific command is passed to [`ClusterServer::command`].
//!
//! Attributes are reached through their descriptors rather than a dynamic
//! value: a server encodes with
//! [`Attribute::encode`](crate::types::descriptors::Attribute::encode) and
//! decodes with
//! [`Attribute::decode`](crate::types::descriptors::Attribute::decode), so an
//! attribute can only be answered with the type the specification gives it.
//!
//! A cluster that is not served at all is answered by
//! [`UnsupportedClusterResponder`], which belongs last in the handler chain.

use byte::BytesExt;
use byte::ctx;
use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::frame::header::ZclHeader;
use crate::frame::header::command_identifier::CommandIdentifier;
use crate::frame::header::frame_control::FrameControl;
use crate::frame::header::frame_control::FrameType;
use crate::reporting::AttributeReporting;
use crate::reporting::MAX_INTERVAL_DISABLED;
use crate::reporting::MIN_INTERVAL_DEFAULT;
use crate::reporting::ReportingConfig;
use crate::types::data_types::ZclDataType;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::Cluster;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::ids::CommandId;
use crate::types::ids::TypeId;

/// Frame control for a global, server→client response with the default
/// response disabled.
pub(crate) const RESPONSE_FRAME_CONTROL: u8 = 0x18;

/// Direction of a reporting configuration record: the intervals and reportable
/// change of an attribute this device reports (2.5.7.1.4).
const DIRECTION_REPORTED: u8 = 0x00;

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
/// [`cluster`](Self::cluster), [`attributes`](Self::attributes) and
/// [`encode_value`](Self::encode_value) are all a read-only cluster without
/// commands has to provide. Writable attributes override
/// [`decode_value`](Self::decode_value); a cluster with its own commands
/// overrides [`command`](Self::command); a cluster whose attributes are
/// reportable overrides [`reporting`](Self::reporting).
pub trait ClusterServer {
    /// Cluster this server answers for.
    fn cluster(&self) -> Cluster;

    /// Every attribute this server implements, in ascending identifier order.
    ///
    /// This is the list `Discover Attributes` walks (2.5.13.3), and what
    /// `Write Attributes Undivided` and `Configure Reporting` check a record
    /// against before touching any state. Build it from the cluster's
    /// descriptors with
    /// [`Attribute::attr_info`](crate::types::descriptors::Attribute::attr_info)
    /// so it cannot drift from them.
    fn attributes(&self) -> &'static [AttrInfo];

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

    /// Where this server keeps the reporting configurations a coordinator sets
    /// up with `Configure Reporting` (2.5.7).
    ///
    /// The default has nowhere to keep them, so every attribute is answered
    /// `UNREPORTABLE_ATTRIBUTE` rather than accepting a configuration the
    /// device would not honor.
    fn reporting(&self) -> Option<&dyn AttributeReporting> {
        None
    }

    /// Handle a cluster-specific command (2.4.1.1.1).
    ///
    /// The default reports every command as unsupported, which is what a
    /// cluster with only global commands owes the requester.
    fn command(&self, _command: ClusterCommand<'_>, _out: &mut [u8]) -> CommandOutcome {
        CommandOutcome::Status(Status::UnsupCommand)
    }

    /// Route a request to this server, parsing the ZCL header once: the
    /// attribute global commands are answered from the methods above, a
    /// cluster-specific command goes to [`command`](Self::command), and
    /// anything else draws a Default Response.
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
                CommandIdentifier::WriteAttributesUndivided => {
                    write_attributes_undivided(self, header, body, out)
                }
                // 2.5.6.3: no response, error response or Default Response
                CommandIdentifier::WriteAttributesNoResponse => {
                    apply_write_records(self, body);
                    None
                }
                CommandIdentifier::ConfigureReporting => {
                    configure_reporting(self, header, body, out)
                }
                CommandIdentifier::ReadReportingConfiguration => {
                    read_reporting_configuration(self, header, body, out)
                }
                CommandIdentifier::DiscoverAttributes => {
                    discover_attributes(self, header, body, out)
                }
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

/// Answers a request for a cluster that is not served on this endpoint
/// (2.5.12.2).
///
/// The specification owes such a request a Default Response carrying
/// `UNSUPPORTED_CLUSTER`; the cluster servers themselves cannot, since each
/// only recognizes its own cluster. Place this last in the handler chain and
/// give it the cluster identifiers the endpoint does advertise, so a cluster
/// that *is* served but chose to stay silent — a `Write Attributes No
/// Response`, a broadcast — is not contradicted here.
#[derive(Debug, Clone, Copy)]
pub struct UnsupportedClusterResponder<'a> {
    served: &'a [u16],
}

impl<'a> UnsupportedClusterResponder<'a> {
    /// A responder for every cluster outside `served`, which is the endpoint's
    /// own input cluster list.
    pub const fn new(served: &'a [u16]) -> Self {
        Self { served }
    }
}

impl ClusterRequestHandler for UnsupportedClusterResponder<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        if self.served.contains(&request.cluster_id) {
            return None;
        }

        let header: ZclHeader = request.asdu.read_with(&mut 0, ()).ok()?;
        // 2.5.12.2: never in reply to another Default Response
        if header.frame_control.frame_type() == FrameType::GlobalCommand
            && header.command_identifier == CommandIdentifier::DefaultResponse
        {
            return None;
        }

        let len = default_response(header, request.unicast, Status::UnsupportedCluster, out)?;
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

/// Apply one write attribute record (2.5.3.1.1), leaving `offset` past it
/// whether or not the value was accepted.
fn apply_write_record<S>(
    server: &S,
    body: &[u8],
    offset: &mut usize,
) -> Option<(AttributeId, Status)>
where
    S: ClusterServer + ?Sized,
{
    let id: u16 = body.read_with(offset, ctx::LE).ok()?;
    let raw_type: u8 = body.read_with(offset, ctx::LE).ok()?;

    let value_at = *offset;
    let status = server.decode_value(AttributeId(id), TypeId::from_u8(raw_type), body, offset);
    if status != Status::Success {
        // the server left the offset untouched, so step over the value with
        // the length its own type byte implies to stay aligned on the next
        // record
        *offset = value_at;
        skip_value(raw_type, body, offset)?;
    }

    Some((AttributeId(id), status))
}

/// Apply every write attribute record in `body`, discarding the statuses —
/// what `Write Attributes No Response` asks for (2.5.6.3).
fn apply_write_records<S>(server: &S, body: &[u8])
where
    S: ClusterServer + ?Sized,
{
    let offset = &mut 0;
    while *offset < body.len() {
        if apply_write_record(server, body, offset).is_none() {
            return;
        }
    }
}

/// Step over an attribute value of the type `raw_type` names, using the type
/// itself as the length oracle.
fn skip_value(raw_type: u8, body: &[u8], offset: &mut usize) -> Option<()> {
    let _: ZclDataType<'_> = body.read_with(offset, raw_type).ok()?;
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
        let (id, status) = apply_write_record(server, body, request)?;
        if status == Status::Success {
            continue;
        }

        failures += 1;
        out.write_with(offset, status, ()).ok()?;
        out.write_with(offset, id.0, ctx::LE).ok()?;
    }

    // all-success is reported as one status record with no attribute id
    if failures == 0 {
        out.write_with(offset, Status::Success, ()).ok()?;
    }

    Some(*offset)
}

/// Answer `Write Attributes Undivided` (2.5.4): if any record cannot be
/// written, none is.
///
/// Every record is checked against the attribute list first, so a rejection is
/// known before any value has been applied. Whether a value is inside the
/// attribute's own valid range is only known to
/// [`ClusterServer::decode_value`], so a record that passes the check here and
/// is refused there still leaves the earlier records applied; the response
/// reports it truthfully.
fn write_attributes_undivided<S>(
    server: &S,
    header: ZclHeader,
    body: &[u8],
    out: &mut [u8],
) -> Option<usize>
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
        skip_value(raw_type, body, request)?;

        let status = check_writable(server, AttributeId(id), TypeId::from_u8(raw_type));
        if status == Status::Success {
            continue;
        }

        failures += 1;
        out.write_with(offset, status, ()).ok()?;
        out.write_with(offset, id, ctx::LE).ok()?;
    }

    if failures > 0 {
        return Some(*offset);
    }

    write_attributes(server, header, body, out)
}

/// Whether a write record could be applied, from the attribute list alone
/// (2.5.3.3).
fn check_writable<S>(server: &S, id: AttributeId, type_id: TypeId) -> Status
where
    S: ClusterServer + ?Sized,
{
    let Some(info) = find_attribute(server, id) else {
        return Status::UnsupportedAttribute;
    };
    if !info.access.is_writable() {
        return Status::ReadOnly;
    }
    if info.type_id != type_id {
        return Status::InvalidDataType;
    }
    Status::Success
}

fn find_attribute<S>(server: &S, id: AttributeId) -> Option<AttrInfo>
where
    S: ClusterServer + ?Sized,
{
    server.attributes().iter().copied().find(|a| a.id == id)
}

/// Answer `Discover Attributes` (2.5.13.3).
///
/// The attribute list is already in ascending identifier order, so the
/// response is the window of it that starts at the requested identifier and is
/// bounded by the requested count and by what fits in the frame.
fn discover_attributes<S>(
    server: &S,
    header: ZclHeader,
    body: &[u8],
    out: &mut [u8],
) -> Option<usize>
where
    S: ClusterServer + ?Sized,
{
    let request = &mut 0;
    let start: u16 = body.read_with(request, ctx::LE).ok()?;
    let maximum: u8 = body.read_with(request, ctx::LE).ok()?;

    let offset = &mut 0;
    write_response_header(
        header.sequence_number,
        CommandIdentifier::DiscoverAttributesResponse,
        out,
        offset,
    )?;

    // patched once the window is known to have covered the whole list
    let complete_at = *offset;
    out.write_with(offset, 0u8, ctx::LE).ok()?;

    let mut remaining = server
        .attributes()
        .iter()
        .filter(|info| info.id.0 >= start)
        .peekable();

    let mut written = 0u8;
    while written < maximum {
        let Some(info) = remaining.peek() else { break };
        let record = *offset;
        if out.write_with(offset, info.id.0, ctx::LE).is_err()
            || out
                .write_with(offset, info.type_id.as_u8(), ctx::LE)
                .is_err()
        {
            *offset = record;
            break;
        }
        remaining.next();
        written += 1;
    }

    // 2.5.14.1.2: complete when nothing was left behind
    let complete = &mut { complete_at };
    out.write_with(complete, u8::from(remaining.peek().is_none()), ctx::LE)
        .ok()?;

    Some(*offset)
}

/// Answer `Configure Reporting` (2.5.7.3).
///
/// Each record is validated against the attribute list and the accepted ones
/// are stored in [`ClusterServer::reporting`], so what the response promises
/// is what the application later reports on. A response with no failing record
/// carries a single `Success` status with the direction and identifier
/// omitted.
fn configure_reporting<S>(
    server: &S,
    header: ZclHeader,
    body: &[u8],
    out: &mut [u8],
) -> Option<usize>
where
    S: ClusterServer + ?Sized,
{
    let offset = &mut 0;
    write_response_header(
        header.sequence_number,
        CommandIdentifier::ConfigureReportingResponse,
        out,
        offset,
    )?;

    let cluster = server.cluster().id();
    let request = &mut 0;
    let mut failures = 0usize;
    while *request < body.len() {
        let direction: u8 = body.read_with(request, ctx::LE).ok()?;
        let id: u16 = body.read_with(request, ctx::LE).ok()?;

        let status = if direction == DIRECTION_REPORTED {
            configure_one(server, cluster, AttributeId(id), body, request)?
        } else {
            // the timeout period configures reports this device *receives*; a
            // pure server has none (2.5.7.3)
            let _timeout: u16 = body.read_with(request, ctx::LE).ok()?;
            Status::UnreportableAttribute
        };

        if status == Status::Success {
            continue;
        }

        failures += 1;
        out.write_with(offset, status, ()).ok()?;
        out.write_with(offset, direction, ctx::LE).ok()?;
        out.write_with(offset, id, ctx::LE).ok()?;
    }

    if failures == 0 {
        out.write_with(offset, Status::Success, ()).ok()?;
    }

    Some(*offset)
}

/// Validate and store one attribute reporting configuration record, consuming
/// its fields from `body` (2.5.7.1, 2.5.7.3).
fn configure_one<S>(
    server: &S,
    cluster: ClusterId,
    id: AttributeId,
    body: &[u8],
    offset: &mut usize,
) -> Option<Status>
where
    S: ClusterServer + ?Sized,
{
    let raw_type: u8 = body.read_with(offset, ctx::LE).ok()?;
    let record_type = TypeId::from_u8(raw_type);
    let min_interval: u16 = body.read_with(offset, ctx::LE).ok()?;
    let max_interval: u16 = body.read_with(offset, ctx::LE).ok()?;
    // the field is present exactly for analog types, and its width is the
    // attribute type's own (2.5.7.1.7)
    let reportable_change = if record_type.is_analog() {
        Some(read_change(record_type, body, offset)?)
    } else {
        None
    };

    let Some(info) = find_attribute(server, id) else {
        return Some(Status::UnsupportedAttribute);
    };
    if !info.type_id.is_reportable_type() || !info.access.is_reportable() {
        return Some(Status::UnreportableAttribute);
    }
    if info.type_id != record_type {
        return Some(Status::InvalidDataType);
    }
    let Some(store) = server.reporting() else {
        return Some(Status::UnreportableAttribute);
    };

    // 2.5.7.1.6: stop reporting, or revert to the default configuration —
    // either way this device keeps no configuration of its own
    if max_interval == MAX_INTERVAL_DISABLED
        || (max_interval == 0 && min_interval == MIN_INTERVAL_DEFAULT)
    {
        store.clear(cluster, id);
        return Some(Status::Success);
    }
    if max_interval != 0 && max_interval < min_interval {
        return Some(Status::InvalidValue);
    }

    let config = ReportingConfig {
        min_interval,
        max_interval,
        reportable_change: reportable_change.unwrap_or(0),
    };
    Some(if store.configure(cluster, id, config) {
        Status::Success
    } else {
        Status::InsufficientSpace
    })
}

/// Read a reportable change field as a magnitude; its sign is ignored
/// (2.5.7.1.7).
fn read_change(type_id: TypeId, body: &[u8], offset: &mut usize) -> Option<u64> {
    let width = type_id.fixed_size()?;
    let raw = body.get(*offset..*offset + width)?;
    *offset += width;

    let mut bytes = [0u8; 8];
    bytes[..width].copy_from_slice(raw);
    let value = u64::from_le_bytes(bytes);

    Some(match type_id {
        // sign-extend from the narrow width, then drop the sign
        TypeId::Int8
        | TypeId::Int16
        | TypeId::Int24
        | TypeId::Int32
        | TypeId::Int40
        | TypeId::Int48
        | TypeId::Int56
        | TypeId::Int64 => {
            let shift = 64 - width * 8;
            #[allow(clippy::cast_possible_wrap)]
            let signed = ((value as i64) << shift) >> shift;
            signed.unsigned_abs()
        }
        // a floating point change cannot be compared as an integer; treat it
        // as "report every change" rather than silently mis-scaling it
        TypeId::SemiPrecision | TypeId::SinglePrecision | TypeId::DoublePrecision => 0,
        _ => value,
    })
}

/// Answer `Read Reporting Configuration` (2.5.9.2), echoing back what
/// `Configure Reporting` accepted.
fn read_reporting_configuration<S>(
    server: &S,
    header: ZclHeader,
    body: &[u8],
    out: &mut [u8],
) -> Option<usize>
where
    S: ClusterServer + ?Sized,
{
    let offset = &mut 0;
    write_response_header(
        header.sequence_number,
        CommandIdentifier::ReadReportingConfigurationResponse,
        out,
        offset,
    )?;

    let cluster = server.cluster().id();
    let request = &mut 0;
    while *request < body.len() {
        let direction: u8 = body.read_with(request, ctx::LE).ok()?;
        let id: u16 = body.read_with(request, ctx::LE).ok()?;

        let record = *offset;
        if write_reporting_configuration_record(
            server,
            cluster,
            direction,
            AttributeId(id),
            out,
            offset,
        )
        .is_none()
        {
            // 2.5.10.2: only as many records as fit are returned
            *offset = record;
            break;
        }
    }

    Some(*offset)
}

/// Write one attribute reporting configuration record (2.5.10.1); a record
/// that failed carries the status, direction and identifier alone.
fn write_reporting_configuration_record<S>(
    server: &S,
    cluster: ClusterId,
    direction: u8,
    id: AttributeId,
    out: &mut [u8],
    offset: &mut usize,
) -> Option<()>
where
    S: ClusterServer + ?Sized,
{
    let info = find_attribute(server, id);
    let config = server
        .reporting()
        .and_then(|store| store.configuration(cluster, id));

    let status = match info {
        None => Status::UnsupportedAttribute,
        Some(_) if direction != DIRECTION_REPORTED => Status::UnreportableAttribute,
        Some(info) if !info.access.is_reportable() => Status::UnreportableAttribute,
        // nothing was configured for an attribute that could have been
        Some(_) if config.is_none() => Status::NotFound,
        Some(_) => Status::Success,
    };

    out.write_with(offset, status, ()).ok()?;
    out.write_with(offset, direction, ctx::LE).ok()?;
    out.write_with(offset, id.0, ctx::LE).ok()?;

    let (Status::Success, Some(info), Some(config)) = (status, info, config) else {
        return Some(());
    };

    out.write_with(offset, info.type_id.as_u8(), ctx::LE).ok()?;
    out.write_with(offset, config.min_interval, ctx::LE).ok()?;
    out.write_with(offset, config.max_interval, ctx::LE).ok()?;
    if info.type_id.is_analog() {
        let width = info.type_id.fixed_size()?;
        let bytes = config.reportable_change.to_le_bytes();
        out.get_mut(*offset..*offset + width)?
            .copy_from_slice(&bytes[..width]);
        *offset += width;
    }

    Some(())
}

/// Build a Default Response (2.5.12) for the command `header` describes.
///
/// Returns `None` when the spec forbids one (2.5.12.2): the request was not
/// unicast, or it disabled the default response and the command was carried
/// out — an error is reported whether or not the bit is set. `status` reports
/// how the command was handled — `UnsupCommand` for an unrecognized command
/// id, `UnsupportedCluster` for a cluster the endpoint does not serve.
fn default_response(
    header: ZclHeader,
    unicast: bool,
    status: Status,
    out: &mut [u8],
) -> Option<usize> {
    if !unicast {
        return None;
    }
    if header.frame_control.disable_default_response() && status == Status::Success {
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
    use crate::clusters::general::basic;
    use crate::clusters::general::basic::BasicServer;
    use crate::clusters::general::identify::IdentifyServer;
    use crate::clusters::measurement::temperature;
    use crate::clusters::measurement::temperature::TemperatureMeasurementServer;
    use crate::reporting::ReportingTable;

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

    // 2.5.6.3: no response of any kind, but the write still happens
    #[test]
    fn write_attributes_no_response_applies_without_answering() {
        let server = IdentifyServer::new();
        let asdu = [0x00, 0x07, 0x05, 0x00, 0x00, 0x21, 0x0a, 0x00];

        let mut out = [0u8; 32];
        assert_eq!(
            server.handle_request(&request(0x0003, &asdu), &mut out),
            None
        );
        assert_eq!(server.identify_time(), 10);
    }

    // 2.5.4: if any record cannot be written, none of them is
    #[test]
    fn an_undivided_write_applies_nothing_when_one_record_fails() {
        let server = IdentifyServer::new();
        let asdu = [
            0x00, 0x07, 0x03, // frame control, sequence, WriteAttributesUndivided
            0x00, 0x00, 0x21, 0x0a, 0x00, // IdentifyTime, uint16, 10
            0x99, 0x99, 0x20, 0x01, // unsupported attribute, uint8, 1
        ];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(0x0003, &asdu), &mut out)
            .expect("handled");

        assert_eq!(server.identify_time(), 0);
        // only the failing record is reported (2.5.4.1.2)
        assert_eq!(&out[..reply.len], &[0x18, 0x07, 0x04, 0x86, 0x99, 0x99]);
    }

    #[test]
    fn an_undivided_write_applies_everything_when_all_records_pass() {
        let server = IdentifyServer::new();
        let asdu = [0x00, 0x07, 0x03, 0x00, 0x00, 0x21, 0x0a, 0x00];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(0x0003, &asdu), &mut out)
            .expect("handled");

        assert_eq!(server.identify_time(), 10);
        assert_eq!(&out[..reply.len], &[0x18, 0x07, 0x04, 0x00]);
    }

    // 2.5.13.3 / 2.5.14.1: identifiers and types in ascending order, with a
    // discovery-complete flag
    #[test]
    fn discover_attributes_walks_the_attribute_list() {
        let asdu = [0x00, 0x0b, 0x0c, 0x00, 0x00, 0xff];

        let mut out = [0u8; 64];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(
            &out[..reply.len],
            &[
                0x18, 0x0b, 0x0d, // frame control, sequence, DiscoverAttributesResponse
                0x01, // discovery complete
                0x00, 0x00, 0x20, // ZCLVersion, uint8
                0x01, 0x00, 0x20, // ApplicationVersion, uint8
                0x02, 0x00, 0x20, // StackVersion, uint8
                0x03, 0x00, 0x20, // HWVersion, uint8
                0x04, 0x00, 0x42, // ManufacturerName, string
                0x05, 0x00, 0x42, // ModelIdentifier, string
                0x07, 0x00, 0x30, // PowerSource, enum8
            ]
        );
    }

    // 2.5.14.1.2: complete is 0 while attributes past the window remain
    #[test]
    fn discover_attributes_honors_the_start_and_the_maximum() {
        let asdu = [0x00, 0x0b, 0x0c, 0x04, 0x00, 0x02];

        let mut out = [0u8; 64];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(
            &out[..reply.len],
            &[0x18, 0x0b, 0x0d, 0x00, 0x04, 0x00, 0x42, 0x05, 0x00, 0x42]
        );
    }

    // 2.5.7.3: the accepted configuration is the one the store now holds
    #[test]
    fn configure_reporting_stores_what_it_accepts() {
        static TABLE: ReportingTable<2> = ReportingTable::new();
        let server = TemperatureMeasurementServer::new(-4000, 12500).with_reporting(&TABLE);
        // MeasuredValue, int16, min 30 s, max 600 s, reportable change 50
        let asdu = [
            0x00, 0x2b, 0x06, // frame control, sequence, ConfigureReporting
            0x00, 0x00, 0x00, 0x29, 0x1e, 0x00, 0x58, 0x02, 0x32, 0x00,
        ];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(temperature::CLUSTER_ID, &asdu), &mut out)
            .expect("handled");

        assert_eq!(&out[..reply.len], &[0x18, 0x2b, 0x07, 0x00]);
        let config = TABLE
            .configuration(temperature::CLUSTER.id(), temperature::MEASURED_VALUE.id())
            .expect("configuration stored");
        assert_eq!(config.min_interval, 30);
        assert_eq!(config.max_interval, 600);
        assert_eq!(config.reportable_change, 50);
    }

    // 2.5.7.3: an attribute the cluster does not have is refused rather than
    // silently accepted
    #[test]
    fn configure_reporting_refuses_an_unknown_attribute() {
        static TABLE: ReportingTable<2> = ReportingTable::new();
        let server = TemperatureMeasurementServer::new(-4000, 12500).with_reporting(&TABLE);
        let asdu = [
            0x00, 0x2b, 0x06, 0x00, 0x99, 0x99, 0x29, 0x1e, 0x00, 0x58, 0x02, 0x32, 0x00,
        ];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(temperature::CLUSTER_ID, &asdu), &mut out)
            .expect("handled");

        // status UNSUPPORTED_ATTRIBUTE, direction, attribute identifier
        assert_eq!(
            &out[..reply.len],
            &[0x18, 0x2b, 0x07, 0x86, 0x00, 0x99, 0x99]
        );
        assert!(TABLE.is_empty());
    }

    // 2.5.7.3: a non-reportable attribute is refused even though it exists
    #[test]
    fn configure_reporting_refuses_a_non_reportable_attribute() {
        static TABLE: ReportingTable<2> = ReportingTable::new();
        let server = TemperatureMeasurementServer::new(-4000, 12500)
            .with_reporting(&TABLE)
            .with_tolerance(100);
        // Tolerance (0x0003), uint16 — implemented, but not reportable
        let asdu = [
            0x00, 0x2b, 0x06, 0x00, 0x03, 0x00, 0x21, 0x1e, 0x00, 0x58, 0x02, 0x32, 0x00,
        ];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(temperature::CLUSTER_ID, &asdu), &mut out)
            .expect("handled");

        assert_eq!(
            &out[..reply.len],
            &[0x18, 0x2b, 0x07, 0x8c, 0x00, 0x03, 0x00]
        );
    }

    // a server that keeps no reporting configuration says so rather than
    // accepting one it would not honor (2.5.7.3)
    #[test]
    fn configure_reporting_is_refused_without_a_store() {
        let server = TemperatureMeasurementServer::new(-4000, 12500);
        let asdu = [
            0x00, 0x2b, 0x06, 0x00, 0x00, 0x00, 0x29, 0x1e, 0x00, 0x58, 0x02, 0x32, 0x00,
        ];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(temperature::CLUSTER_ID, &asdu), &mut out)
            .expect("handled");

        // UNREPORTABLE_ATTRIBUTE, direction, MeasuredValue
        assert_eq!(
            &out[..reply.len],
            &[0x18, 0x2b, 0x07, 0x8c, 0x00, 0x00, 0x00]
        );
    }

    // 2.5.9.2 / 2.5.10.1: what Configure Reporting accepted comes back
    #[test]
    fn read_reporting_configuration_echoes_the_stored_configuration() {
        static TABLE: ReportingTable<2> = ReportingTable::new();
        let server = TemperatureMeasurementServer::new(-4000, 12500).with_reporting(&TABLE);
        let configure = [
            0x00, 0x2b, 0x06, 0x00, 0x00, 0x00, 0x29, 0x1e, 0x00, 0x58, 0x02, 0x32, 0x00,
        ];
        let mut out = [0u8; 32];
        server
            .handle_request(&request(temperature::CLUSTER_ID, &configure), &mut out)
            .expect("configured");

        // Read Reporting Configuration for MeasuredValue, direction 0x00
        let asdu = [0x00, 0x2c, 0x08, 0x00, 0x00, 0x00];
        let reply = server
            .handle_request(&request(temperature::CLUSTER_ID, &asdu), &mut out)
            .expect("handled");

        assert_eq!(
            &out[..reply.len],
            &[
                0x18, 0x2c, 0x09, // frame control, sequence, response
                0x00, // SUCCESS
                0x00, // direction
                0x00, 0x00, // MeasuredValue
                0x29, // int16
                0x1e, 0x00, // min 30 s
                0x58, 0x02, // max 600 s
                0x32, 0x00, // reportable change 50
            ]
        );
    }

    // 2.5.10: an attribute that could be configured but was not
    #[test]
    fn read_reporting_configuration_reports_an_unconfigured_attribute() {
        static TABLE: ReportingTable<2> = ReportingTable::new();
        let server = TemperatureMeasurementServer::new(-4000, 12500).with_reporting(&TABLE);
        let asdu = [0x00, 0x2c, 0x08, 0x00, 0x00, 0x00];

        let mut out = [0u8; 32];
        let reply = server
            .handle_request(&request(temperature::CLUSTER_ID, &asdu), &mut out)
            .expect("handled");

        // NOT_FOUND, direction, attribute identifier
        assert_eq!(
            &out[..reply.len],
            &[0x18, 0x2c, 0x09, 0x8b, 0x00, 0x00, 0x00]
        );
    }

    // an unrecognized global command draws a Default Response (2.5.12.2)
    #[test]
    fn an_unsupported_global_command_draws_a_default_response() {
        // Read Attributes Structured (0x0e) is not implemented
        let asdu = [0x00, 0x0b, 0x0e, 0x00, 0x00, 0x05];

        let mut out = [0u8; 32];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(&out[..reply.len], &[0x18, 0x0b, 0x0b, 0x0e, 0x81]);
    }

    // 2.5.12.2: a disabled default response is overridden when an error results
    #[test]
    fn an_error_answers_even_with_the_default_response_disabled() {
        let asdu = [0x10, 0x0b, 0x0e, 0x00, 0x00, 0x05];

        let mut out = [0u8; 32];
        let reply = SERVER
            .handle_request(&request(0x0000, &asdu), &mut out)
            .expect("handled");

        assert_eq!(&out[..reply.len], &[0x18, 0x0b, 0x0b, 0x0e, 0x81]);
    }

    // 2.5.12.2: a unicast command for a cluster the endpoint does not serve
    // draws UNSUPPORTED_CLUSTER, which no cluster server can answer itself
    #[test]
    fn an_unserved_cluster_draws_unsupported_cluster() {
        let responder = UnsupportedClusterResponder::new(&[basic::CLUSTER_ID]);
        let asdu = [0x00, 0x2a, 0x00, 0x00, 0x00];

        let mut out = [0u8; 32];
        let reply = responder
            .handle(&request(0x0006, &asdu), &mut out)
            .expect("handled");

        assert_eq!(&out[..reply.len], &[0x18, 0x2a, 0x0b, 0x00, 0xc3]);
    }

    #[test]
    fn a_served_cluster_is_left_to_its_own_server() {
        let responder = UnsupportedClusterResponder::new(&[basic::CLUSTER_ID]);
        let asdu = [0x00, 0x2a, 0x00, 0x00, 0x00];

        let mut out = [0u8; 32];
        assert_eq!(responder.handle(&request(0x0000, &asdu), &mut out), None);
    }

    // 2.5.12.2: never in reply to a Default Response, and never to a broadcast
    #[test]
    fn an_unserved_cluster_stays_silent_where_the_spec_forbids_a_response() {
        let responder = UnsupportedClusterResponder::new(&[basic::CLUSTER_ID]);
        let mut out = [0u8; 32];

        let default_response = [0x18, 0x2a, 0x0b, 0x00, 0x00];
        assert_eq!(
            responder.handle(&request(0x0006, &default_response), &mut out),
            None
        );

        let asdu = [0x00, 0x2a, 0x00, 0x00, 0x00];
        let broadcast = ClusterRequest {
            unicast: false,
            ..request(0x0006, &asdu)
        };
        assert_eq!(responder.handle(&broadcast, &mut out), None);
    }
}
