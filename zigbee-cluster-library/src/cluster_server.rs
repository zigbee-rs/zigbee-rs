use crate::frame::Direction;
use crate::frame::IncomingGlobalCommand;
use crate::frame::IncomingZclCommand;
use crate::frame::IncomingZclFrame;
use crate::frame::OutgoingGlobalCommand;
use crate::frame::OutgoingZclFrame;
use crate::frame::Status;
use crate::frame::ZclFrameMeta;
use crate::payload::WriteAttrParseErr;
use crate::payload::WriteAttributesPayload;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::ClusterKey;
use crate::types::error::AttrError;
use crate::types::error::ZclError;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::ids::CommandId;
use crate::types::ids::ManufacturerCode;
use crate::types::ids::TypeId;

// Dispatch context

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchContext {
    pub delivery: DeliveryMode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryMode {
    Unicast,
    BroadcastOrMulticast,
}

impl DispatchContext {
    pub const fn allows_default_response(self) -> bool {
        matches!(self.delivery, DeliveryMode::Unicast)
    }
}

// CommandResult

#[derive(Clone, Copy)]
pub enum CommandResult {
    /// Cluster impl returns a status; dispatcher builds `DefaultResponse`.
    DefaultResponse(Status),
    /// Cluster impl wrote `len` bytes of payload into buf; dispatcher prepends
    /// ZCL header.
    Payload { command_id: CommandId, len: usize },
    /// Unconditionally suppress any response.
    Suppress,
}

// ClusterServer trait

pub trait ClusterServer {
    const CLUSTER_ID: ClusterId;

    fn read_attribute(&self, id: AttributeId, buf: &mut [u8])
    -> Result<(TypeId, usize), AttrError>;

    /// Validate an incoming write without mutating state. Used by
    /// `WriteAttributesUndivided`.
    fn check_write_attribute(
        &self,
        id: AttributeId,
        type_id: TypeId,
        data: &[u8],
    ) -> Result<(), AttrError>;

    fn write_attribute(
        &mut self,
        id: AttributeId,
        type_id: TypeId,
        data: &[u8],
    ) -> Result<(), AttrError>;

    fn handle_command(
        &mut self,
        frame: &IncomingZclFrame<'_>,
        ctx: DispatchContext,
        buf: &mut [u8],
    ) -> Result<CommandResult, ZclError> {
        let _ = (frame, ctx, buf);
        Ok(CommandResult::DefaultResponse(Status::UnsupCommand))
    }

    /// Visit attribute metadata for `DiscoverAttributes` in ascending id order.
    fn visit_attributes(
        &self,
        visitor: &mut dyn FnMut(AttrInfo) -> Result<(), ZclError>,
    ) -> Result<(), ZclError> {
        let _ = visitor;
        Ok(())
    }

    fn dispatch(
        &mut self,
        frame: &IncomingZclFrame<'_>,
        ctx: DispatchContext,
        buf: &mut [u8],
    ) -> Result<usize, ZclError>
    where
        Self: Sized,
    {
        zcl_cluster_dispatch(self, frame, ctx, buf)
    }
}

// Device trait

pub enum DispatchError {
    UnsupportedCluster,
    Codec(ZclError),
}

impl From<ZclError> for DispatchError {
    fn from(e: ZclError) -> Self {
        Self::Codec(e)
    }
}

pub trait Device {
    fn dispatch_cluster(
        &mut self,
        cluster_id: ClusterId,
        manufacturer_code: Option<ManufacturerCode>,
        ctx: DispatchContext,
        frame: &IncomingZclFrame<'_>,
        buf: &mut [u8],
    ) -> Result<usize, DispatchError>;

    fn server_cluster_ids(&self) -> &'static [ClusterKey];
}

// Public helpers

/// Build a non-manufacturer-specific `DefaultResponse` frame into `buf`.
/// Returns bytes written.
pub fn build_default_response(
    request_command: crate::header::command_identifier::CommandIdentifier,
    status: Status,
    seq: u8,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    build_default_response_with_mfr(request_command, status, seq, None, buf)
}

/// Build a `DefaultResponse` frame for `frame`, preserving
/// manufacturer-specific framing.
pub fn build_default_response_for_frame(
    frame: &IncomingZclFrame<'_>,
    status: Status,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    let Some(response) = OutgoingZclFrame::default_response(frame, status) else {
        return Ok(0);
    };
    response.encode(buf)
}

/// Build a `DefaultResponse` frame into `buf`, optionally preserving a
/// manufacturer code.
pub fn build_default_response_with_mfr(
    request_command: crate::header::command_identifier::CommandIdentifier,
    status: Status,
    seq: u8,
    manufacturer_code: Option<ManufacturerCode>,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    let mut meta = ZclFrameMeta::new(seq, Direction::ServerToClient).disable_default_response();
    if let Some(code) = manufacturer_code {
        meta = meta.with_manufacturer_code(code);
    }
    OutgoingZclFrame::global(
        meta,
        OutgoingGlobalCommand::DefaultResponse(crate::frame::DefaultResponse {
            command_identifier: request_command.raw(),
            status,
        }),
    )
    .encode(buf)
}

/// True when ZCL rules allow sending a `DefaultResponse` for this frame +
/// context
/// + status.
pub fn should_send_default_response(
    frame: &IncomingZclFrame<'_>,
    ctx: DispatchContext,
    status: Status,
) -> bool {
    if !ctx.allows_default_response() {
        return false;
    }
    if matches!(
        frame.command(),
        IncomingZclCommand::Global(IncomingGlobalCommand::DefaultResponse(_))
            | IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributesNoResponse(_))
    ) {
        return false;
    }
    if status == Status::Success && frame.disable_default_response() {
        return false;
    }
    true
}

// Internal helpers

fn response_header_len(manufacturer_code: Option<ManufacturerCode>) -> usize {
    3 + if manufacturer_code.is_some() { 2 } else { 0 }
}

fn zcl_response_header_len(frame: &IncomingZclFrame<'_>) -> usize {
    response_header_len(frame.manufacturer_code())
}

fn write_response_header_parts(
    buf: &mut [u8],
    frame: &IncomingZclFrame<'_>,
    cmd_id: u8,
    frame_control_base: u8,
) -> Result<usize, ZclError> {
    let manufacturer_code = frame.manufacturer_code();
    let mfr_bit = if manufacturer_code.is_some() {
        0x04u8
    } else {
        0x00u8
    };
    let needed = response_header_len(manufacturer_code);
    if buf.len() < needed {
        return Err(ZclError::BufferTooSmall);
    }
    let mut n = 0;
    buf[n] = frame_control_base | mfr_bit;
    n += 1;
    if let Some(mfr) = manufacturer_code {
        buf[n..n + 2].copy_from_slice(&mfr.0.to_le_bytes());
        n += 2;
    }
    buf[n] = frame.sequence_number();
    n += 1;
    buf[n] = cmd_id;
    n += 1;
    Ok(n)
}

fn write_global_response_header(
    buf: &mut [u8],
    frame: &IncomingZclFrame<'_>,
    cmd_id: u8,
) -> Result<usize, ZclError> {
    write_response_header_parts(buf, frame, cmd_id, 0x18)
}

fn write_cluster_response_header(
    buf: &mut [u8],
    frame: &IncomingZclFrame<'_>,
    cmd_id: u8,
) -> Result<usize, ZclError> {
    write_response_header_parts(buf, frame, cmd_id, 0x19)
}

fn put_byte(buf: &mut [u8], pos: usize, val: u8) -> Result<usize, ZclError> {
    *buf.get_mut(pos).ok_or(ZclError::BufferTooSmall)? = val;
    Ok(pos + 1)
}

fn put_status(buf: &mut [u8], pos: usize, status: Status) -> Result<usize, ZclError> {
    let written = status.encode(buf.get_mut(pos..).ok_or(ZclError::BufferTooSmall)?)?;
    Ok(pos + written)
}

fn put_u16_le(buf: &mut [u8], pos: usize, val: u16) -> Result<usize, ZclError> {
    buf.get_mut(pos..pos + 2)
        .ok_or(ZclError::BufferTooSmall)?
        .copy_from_slice(&val.to_le_bytes());
    Ok(pos + 2)
}

fn extract_codec_err(e: AttrError) -> ZclError {
    match e {
        AttrError::Codec(ze) => ze,
        _ => ZclError::InvalidValue,
    }
}

fn put_write_parse_failure(
    buf: &mut [u8],
    pos: usize,
    id: AttributeId,
    error: ZclError,
) -> Result<usize, ZclError> {
    match error {
        ZclError::InvalidValue => {
            let pos = put_status(buf, pos, Status::InvalidDataType)?;
            put_u16_le(buf, pos, id.0)
        }
        other => Err(other),
    }
}

// Main dispatcher

pub fn zcl_cluster_dispatch<CS: ClusterServer>(
    server: &mut CS,
    frame: &IncomingZclFrame<'_>,
    ctx: DispatchContext,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    if frame.direction() == Direction::ServerToClient
        && matches!(
            frame.command(),
            IncomingZclCommand::Global(
                IncomingGlobalCommand::WriteAttributes(_)
                    | IncomingGlobalCommand::WriteAttributesUndivided(_)
                    | IncomingGlobalCommand::WriteAttributesNoResponse(_)
            )
        )
    {
        return Ok(0);
    }

    match frame.command() {
        IncomingZclCommand::Global(IncomingGlobalCommand::ReadAttributes(attrs)) => {
            let hdr_len = write_global_response_header(buf, frame, 0x01)?;
            let mut pos = hdr_len;
            for record in attrs {
                let attr_id = AttributeId::new(record.attribute_id);
                pos = put_u16_le(buf, pos, record.attribute_id)?;
                // Reserve pos for status and pos+1 for type_id; pass [pos+2..] to
                // read_attribute.
                let value_buf = buf.get_mut(pos + 2..).ok_or(ZclError::BufferTooSmall)?;
                match server.read_attribute(attr_id, value_buf) {
                    Ok((type_id, n)) => {
                        buf[pos] = 0x00; // Success
                        buf[pos + 1] = type_id.as_u8();
                        pos += 2 + n;
                    }
                    Err(e) => match e.to_status() {
                        Some(status) => {
                            pos = put_status(buf, pos, status)?;
                        }
                        None => return Err(extract_codec_err(e)),
                    },
                }
            }
            Ok(pos)
        }

        IncomingZclCommand::Global(IncomingGlobalCommand::DefaultResponse(_)) => {
            Ok(0) // ZCL §2.5.12: never respond to an incoming DefaultResponse
        }

        IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributes(payload)) => {
            dispatch_write_attributes(server, payload, frame, ctx, buf)
        }

        IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributesUndivided(payload)) => {
            dispatch_write_attributes_undivided(server, payload, frame, buf)
        }

        IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributesNoResponse(payload)) => {
            for record_result in payload.records() {
                let Ok(record) = record_result else {
                    break;
                };
                let _ = server.write_attribute(record.attr_id, record.type_id, record.value);
            }
            Ok(0) // always Ok(0): no response, not even DefaultResponse (ZCL §2.5.7)
        }

        IncomingZclCommand::Global(IncomingGlobalCommand::DiscoverAttributes {
            start_attr,
            max_count,
        }) => dispatch_discover_attributes::<CS>(server, start_attr.0, *max_count, frame, buf),

        IncomingZclCommand::ClusterSpecific { .. } => {
            if frame.direction() == Direction::ServerToClient {
                // direction=1 means server-to-client: a server ignores these
                return Ok(0);
            }
            let hdr_len = zcl_response_header_len(frame);
            if buf.len() < hdr_len {
                return Err(ZclError::BufferTooSmall);
            }
            let result = server.handle_command(frame, ctx, &mut buf[hdr_len..])?;
            finalize_command_response(result, frame, ctx, hdr_len, buf)
        }

        IncomingZclCommand::Global(
            IncomingGlobalCommand::DiscoverCommandsReceived { .. }
            | IncomingGlobalCommand::DiscoverCommandsGenerated { .. }
            | IncomingGlobalCommand::DiscoverAttributesExtended { .. }
            | IncomingGlobalCommand::KnownUnhandled { .. }
            | IncomingGlobalCommand::Unknown { .. },
        ) => dispatch_unknown_global(frame, ctx, buf),
    }
}

// Sub-dispatchers

fn dispatch_write_attributes<CS: ClusterServer>(
    server: &mut CS,
    payload: &WriteAttributesPayload<'_>,
    frame: &IncomingZclFrame<'_>,
    ctx: DispatchContext,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    let hdr_len = write_global_response_header(buf, frame, 0x04)?;
    let mut pos = hdr_len;

    for record_result in payload.records() {
        match record_result {
            Err(WriteAttrParseErr {
                attr_id: Some(id),
                error,
            }) => {
                pos = put_write_parse_failure(buf, pos, id, error)?;
                break;
            }
            Err(WriteAttrParseErr {
                attr_id: None,
                error,
            }) => {
                return Err(error);
            }
            Ok(record) => {
                match server.write_attribute(record.attr_id, record.type_id, record.value) {
                    Ok(()) => {}
                    Err(e) => match e.to_status() {
                        Some(status) => {
                            pos = put_status(buf, pos, status)?;
                            pos = put_u16_le(buf, pos, record.attr_id.0)?;
                        }
                        None => return Err(extract_codec_err(e)),
                    },
                }
            }
        }
    }

    // pos == hdr_len means all records succeeded → single success (no attr_id)
    if pos == hdr_len {
        pos = put_byte(buf, pos, 0x00)?;
    }

    let _ = ctx; // WriteAttributesResponse never triggers an additional DefaultResponse
    Ok(pos)
}

fn dispatch_write_attributes_undivided<CS: ClusterServer>(
    server: &mut CS,
    payload: &WriteAttributesPayload<'_>,
    frame: &IncomingZclFrame<'_>,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    let hdr_len = write_global_response_header(buf, frame, 0x04)?;

    // First pass: check all records without mutating state. Failure records can
    // stream straight into the response buffer because no writes are committed
    // until the pass completes cleanly.
    let mut failure_pos = hdr_len;
    let mut has_failures = false;
    for record_result in payload.records() {
        match record_result {
            Err(WriteAttrParseErr {
                attr_id: Some(id),
                error,
            }) => {
                failure_pos = put_write_parse_failure(buf, failure_pos, id, error)?;
                has_failures = true;
                break; // stream position irrecoverable; failures non-empty → second pass skipped
            }
            Err(WriteAttrParseErr {
                attr_id: None,
                error,
            }) => {
                return Err(error);
            }
            Ok(record) => {
                if let Err(e) =
                    server.check_write_attribute(record.attr_id, record.type_id, record.value)
                {
                    match e.to_status() {
                        Some(status) => {
                            failure_pos = put_status(buf, failure_pos, status)?;
                            failure_pos = put_u16_le(buf, failure_pos, record.attr_id.0)?;
                            has_failures = true;
                        }
                        None => return Err(extract_codec_err(e)),
                    }
                }
            }
        }
    }

    if has_failures {
        return Ok(failure_pos);
    }

    // Second pass: all checks passed → commit all writes.
    for record_result in payload.records() {
        let record = record_result.map_err(|e| e.error)?;
        // Errors here are unexpected (we just validated), but propagate Codec failures.
        if let Err(AttrError::Codec(ze)) =
            server.write_attribute(record.attr_id, record.type_id, record.value)
        {
            return Err(ze);
        }
    }

    let pos = put_status(buf, hdr_len, Status::Success)?;
    Ok(pos)
}

fn dispatch_discover_attributes<CS: ClusterServer>(
    server: &CS,
    start_attr: u16,
    max_count: u8,
    frame: &IncomingZclFrame<'_>,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    let hdr_len = write_global_response_header(buf, frame, 0x0d)?;
    let discovery_pos = hdr_len;
    let mut pos = put_byte(buf, discovery_pos, 0)?;
    let record_capacity = buf.len().saturating_sub(pos) / 3;
    let max_records = usize::from(max_count).min(record_capacity);
    let mut emitted = 0usize;
    let mut complete = true;

    server.visit_attributes(&mut |attr| {
        if attr.id.0 < start_attr {
            return Ok(());
        }
        if emitted >= max_records {
            complete = false;
            return Ok(());
        }
        pos = put_u16_le(buf, pos, attr.id.0)?;
        pos = put_byte(buf, pos, attr.type_id.as_u8())?;
        emitted += 1;
        Ok(())
    })?;

    buf[discovery_pos] = u8::from(complete);
    Ok(pos)
}

fn dispatch_unknown_global(
    frame: &IncomingZclFrame<'_>,
    ctx: DispatchContext,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    if should_send_default_response(frame, ctx, Status::UnsupCommand) {
        build_default_response_for_frame(frame, Status::UnsupCommand, buf)
    } else {
        Ok(0)
    }
}

fn finalize_command_response(
    result: CommandResult,
    frame: &IncomingZclFrame<'_>,
    ctx: DispatchContext,
    hdr_len: usize,
    buf: &mut [u8],
) -> Result<usize, ZclError> {
    match result {
        CommandResult::Suppress => Ok(0),
        CommandResult::Payload { command_id, len } => {
            // Payload is already in buf[hdr_len..hdr_len+len]. Write header into
            // buf[..hdr_len].
            if len > buf.len().saturating_sub(hdr_len) {
                return Err(ZclError::BufferTooSmall);
            }
            write_cluster_response_header(&mut buf[..hdr_len], frame, command_id.0)?;
            Ok(hdr_len + len)
        }
        CommandResult::DefaultResponse(status) => {
            if should_send_default_response(frame, ctx, status) {
                build_default_response_for_frame(frame, status, buf)
            } else {
                Ok(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use super::*;
    use crate::attribute_store::AttrDescriptor;
    use crate::attribute_store::SplitAttributeStore;
    use crate::attribute_store::StorageKind;
    use crate::frame::IncomingZclFrame;
    use crate::types::descriptors::AccessFlags;

    static TEST_ATTRS: &[AttrDescriptor] = &[
        AttrDescriptor {
            attr: AttributeId::new(0x0000),
            access: AccessFlags::READ,
            type_id: TypeId::Uint8,
            storage: StorageKind::ConstScalar(42),
        },
        AttrDescriptor {
            attr: AttributeId::new(0x0001),
            access: AccessFlags::READ_WRITE,
            type_id: TypeId::Uint16,
            storage: StorageKind::MutableScalar { index: 0 },
        },
        AttrDescriptor {
            attr: AttributeId::new(0x0002),
            access: AccessFlags::READ,
            type_id: TypeId::CharacterString,
            storage: StorageKind::StaticString(crate::attribute_store::StaticStringValue::Text(
                "hello",
            )),
        },
    ];

    const _: () = assert!(
        crate::attribute_store::is_sorted(TEST_ATTRS),
        "TEST_ATTRS must be sorted"
    );
    const _: () = assert!(
        crate::attribute_store::has_no_duplicate_keys(TEST_ATTRS),
        "TEST_ATTRS must have unique keys"
    );

    use core::cell::Cell;

    struct TestServer {
        store: SplitAttributeStore<1>,
    }

    impl TestServer {
        fn new() -> Self {
            Self {
                store: SplitAttributeStore::new(TEST_ATTRS, [Cell::new(0u64)]),
            }
        }
    }

    impl ClusterServer for TestServer {
        const CLUSTER_ID: ClusterId = ClusterId::new(0xABCD);

        fn read_attribute(
            &self,
            id: AttributeId,
            buf: &mut [u8],
        ) -> Result<(TypeId, usize), AttrError> {
            self.store.read_into(id, buf)
        }

        fn check_write_attribute(
            &self,
            id: AttributeId,
            type_id: TypeId,
            data: &[u8],
        ) -> Result<(), AttrError> {
            self.store.check_write_from(id, type_id, data)
        }

        fn write_attribute(
            &mut self,
            id: AttributeId,
            type_id: TypeId,
            data: &[u8],
        ) -> Result<(), AttrError> {
            self.store.write_from(id, type_id, data)
        }

        fn visit_attributes(
            &self,
            visitor: &mut dyn FnMut(AttrInfo) -> Result<(), ZclError>,
        ) -> Result<(), ZclError> {
            self.store.visit_attributes(visitor)
        }
    }

    struct PayloadServer;

    impl ClusterServer for PayloadServer {
        const CLUSTER_ID: ClusterId = ClusterId::new(0xBEEF);

        fn read_attribute(
            &self,
            _id: AttributeId,
            _buf: &mut [u8],
        ) -> Result<(TypeId, usize), AttrError> {
            Err(AttrError::UnsupportedAttribute)
        }

        fn check_write_attribute(
            &self,
            _id: AttributeId,
            _type_id: TypeId,
            _data: &[u8],
        ) -> Result<(), AttrError> {
            Err(AttrError::UnsupportedAttribute)
        }

        fn write_attribute(
            &mut self,
            _id: AttributeId,
            _type_id: TypeId,
            _data: &[u8],
        ) -> Result<(), AttrError> {
            Err(AttrError::UnsupportedAttribute)
        }

        fn handle_command(
            &mut self,
            frame: &IncomingZclFrame<'_>,
            _ctx: DispatchContext,
            buf: &mut [u8],
        ) -> Result<CommandResult, ZclError> {
            let Some(id) = frame.cluster_command_id() else {
                return Ok(CommandResult::DefaultResponse(Status::UnsupCommand));
            };
            if id == CommandId::new(0x40) {
                if buf.len() < 2 {
                    return Err(ZclError::BufferTooSmall);
                }
                buf[0] = 0xAA;
                buf[1] = 0xBB;
                Ok(CommandResult::Payload {
                    command_id: CommandId::new(0x41),
                    len: 2,
                })
            } else {
                Ok(CommandResult::DefaultResponse(Status::UnsupCommand))
            }
        }
    }

    fn unicast() -> DispatchContext {
        DispatchContext {
            delivery: DeliveryMode::Unicast,
        }
    }

    fn broadcast() -> DispatchContext {
        DispatchContext {
            delivery: DeliveryMode::BroadcastOrMulticast,
        }
    }

    // ReadAttributes

    #[test]
    fn read_attributes_success_encodes_value() {
        // ReadAttributes for attr 0x0000 (Uint8 = 42)
        let req: &[u8] = &[
            0x00, // frame control: global, client→server
            0x01, // seq
            0x00, // ReadAttributes
            0x00, 0x00, // attr 0x0000
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // Response: ZCL header(3) + attr_id(2) + status(1) + type_id(1) + value(1) = 8
        assert_eq!(n, 8);
        assert_eq!(buf[0], 0x18); // frame control
        assert_eq!(buf[1], 0x01); // seq echoed
        assert_eq!(buf[2], 0x01); // ReadAttributesResponse
        assert_eq!(buf[3..5], [0x00, 0x00]); // attr_id LE
        assert_eq!(buf[5], 0x00); // Success
        assert_eq!(buf[6], TypeId::Uint8.as_u8());
        assert_eq!(buf[7], 42); // value
    }

    #[test]
    fn read_attributes_unknown_attr_returns_per_record_status() {
        let req: &[u8] = &[
            0x00, 0x02, 0x00, 0xFF, 0xFF, // unknown attr
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // header(3) + attr_id(2) + status(1) = 6
        assert_eq!(n, 6);
        assert_eq!(buf[3..5], [0xFF, 0xFF]);
        assert_eq!(buf[5], Status::UnsupportedAttribute as u8);
    }

    // WriteAttributes

    #[test]
    fn write_attributes_success_returns_single_success_record() {
        // Write attr 0x0001 (Uint16, writable) = 0x1234
        let req: &[u8] = &[
            0x00, 0x03, 0x02, // header: global, seq=3, WriteAttributes
            0x01, 0x00, // attr_id
            0x21, // Uint16
            0x34, 0x12, // 0x1234 LE
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // header(3) + success(1) = 4
        assert_eq!(n, 4);
        assert_eq!(buf[2], 0x04); // WriteAttributesResponse
        assert_eq!(buf[3], 0x00); // Success, no attr_id
    }

    #[test]
    fn write_attributes_readonly_returns_failure_record() {
        // Write attr 0x0000 (read-only)
        let req: &[u8] = &[
            0x00, 0x04, 0x02, 0x00, 0x00, // attr_id 0x0000
            0x20, // Uint8
            0x05,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // header(3) + status(1) + attr_id(2) = 6
        assert_eq!(n, 6);
        assert_eq!(buf[3], Status::ReadOnly as u8);
        assert_eq!(buf[4..6], [0x00, 0x00]);
    }

    #[test]
    fn write_attributes_unknown_type_id_returns_invalid_data_type_record() {
        // attr_id = 0x0001, type_id = 0xFF (Unknown) — value length is unknowable
        let req: &[u8] = &[
            0x00, 0x10, 0x02, // WriteAttributes
            0x01, 0x00, // attr_id
            0xFF, // Unknown type_id — no value bytes follow
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // header(3) + status(1) + attr_id(2) = 6
        assert_eq!(n, 6);
        assert_eq!(buf[2], 0x04); // WriteAttributesResponse
        assert_eq!(buf[3], Status::InvalidDataType as u8);
        assert_eq!(buf[4..6], [0x01, 0x00]); // attr_id LE
    }

    #[test]
    fn write_attributes_no_response_returns_ok_zero() {
        let req: &[u8] = &[
            0x00, 0x05, 0x05, // WriteAttributesNoResponse
            0x00, 0x00, // read-only attr
            0x20, 0x01,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(n, 0);
    }

    // WriteAttributesUndivided

    #[test]
    fn write_attributes_undivided_all_pass_writes_all() {
        // Two records: attr 0x0001 (writable) twice — both should succeed
        let req: &[u8] = &[
            0x00, 0x06, 0x03, // WriteAttributesUndivided
            0x01, 0x00, 0x21, 0x01, 0x00, // attr 0x0001 = 1
            0x01, 0x00, 0x21, 0x02, 0x00, // attr 0x0001 = 2 (second write wins)
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        assert_eq!(n, 4); // header(3) + success(1)
        assert_eq!(buf[3], 0x00);
    }

    #[test]
    fn write_attributes_undivided_any_fail_writes_none() {
        // Record 1: writable attr. Record 2: read-only attr.
        // Both must fail → value unchanged after dispatch.
        let req: &[u8] = &[
            0x00, 0x07, 0x03, 0x01, 0x00, 0x21, 0x99, 0x00, // writable
            0x00, 0x00, 0x20, 0x01, // read-only attr 0x0000
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // Should contain only the failure record for attr 0x0000
        assert!(n > 4);
        let found_readonly = buf[3..n].chunks(3).any(|chunk| {
            chunk.len() == 3
                && chunk[0] == Status::ReadOnly as u8
                && u16::from_le_bytes([chunk[1], chunk[2]]) == 0x0000
        });
        assert!(
            found_readonly,
            "expected ReadOnly failure record for attr 0x0000"
        );

        // Confirm the writable attr was NOT written (value unchanged = 0)
        let mut rbuf = [0u8; 4];
        server
            .store
            .read_into(AttributeId::new(0x0001), &mut rbuf)
            .unwrap();
        assert_eq!(u16::from_le_bytes([rbuf[0], rbuf[1]]), 0u16);
    }

    // DefaultResponse (incoming)

    #[test]
    fn incoming_default_response_returns_ok_zero() {
        let req: &[u8] = &[
            0x18, // server→client, disable DR
            0x08, 0x0b, // DefaultResponse command
            0x00, // responding to ReadAttributes
            0x00, // Success
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn discover_attributes_all_fit_returns_complete() {
        let req: &[u8] = &[
            0x00, 0x09, 0x0c, // DiscoverAttributes
            0x00, 0x00, // start_attr = 0x0000
            0xFF, // max_count = 255
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 64];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        // header(3) + discovery_complete(1) + 3 records × 3 bytes = 13
        assert_eq!(n, 13);
        assert_eq!(buf[2], 0x0d); // DiscoverAttributesResponse
        assert_eq!(buf[3], 0x01); // discovery_complete = true
        // First record: attr 0x0000, Uint8
        assert_eq!(buf[4..6], [0x00, 0x00]);
        assert_eq!(buf[6], TypeId::Uint8.as_u8());
    }

    #[test]
    fn discover_attributes_truncated_returns_incomplete() {
        let req: &[u8] = &[
            0x00, 0x0a, 0x0c, 0x00, 0x00, // start_attr = 0x0000
            0x01, // max_count = 1
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        assert_eq!(buf[3], 0x00); // discovery_complete = false (more exist)
        // Only 1 record: attr 0x0000, Uint8
        let _ = n;
    }

    // ClusterSpecific direction bit

    #[test]
    fn cluster_specific_server_to_client_returns_ok_zero() {
        // direction bit = 1 (server→client): server ignores it
        let req: &[u8] = &[
            0x09, // cluster-specific | direction=server-to-client
            0x0b, 0x00, // command 0x00
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn cluster_specific_payload_response_uses_cluster_specific_header() {
        let req: &[u8] = &[
            0x01, // cluster-specific | client→server
            0x22, 0x40,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = PayloadServer;
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        assert_eq!(n, 5);
        assert_eq!(buf[0], 0x19);
        assert_eq!(buf[1], 0x22);
        assert_eq!(buf[2], 0x41);
        assert_eq!(&buf[3..5], &[0xAA, 0xBB]);
    }

    // Default Response suppression rules

    #[test]
    fn unknown_global_broadcast_returns_ok_zero() {
        // ConfigureReporting (0x06) — unknown in dispatch; broadcast → no DR
        let req: &[u8] = &[0x00, 0x0c, 0x06];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, broadcast(), &mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn unknown_global_unicast_returns_default_response() {
        // ConfigureReporting on unicast → DefaultResponse(UnsupCommand)
        let req: &[u8] = &[0x00, 0x0d, 0x06];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(buf[2], 0x0b); // DefaultResponse
        assert_eq!(buf[3], 0x06); // echoes ConfigureReporting command id
        assert_eq!(buf[4], Status::UnsupCommand as u8);
    }

    #[test]
    fn unknown_global_manufacturer_specific_default_response_preserves_manufacturer_code() {
        let req: &[u8] = &[
            0x04, // global | manufacturer-specific | client→server
            0x34, 0x12, // manufacturer code
            0x55, // sequence
            0x06, // ConfigureReporting
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        assert_eq!(n, 7);
        assert_eq!(buf[0], 0x1c);
        assert_eq!(&buf[1..3], &[0x34, 0x12]);
        assert_eq!(buf[3], 0x55);
        assert_eq!(buf[4], 0x0b);
        assert_eq!(buf[5], 0x06);
        assert_eq!(buf[6], Status::UnsupCommand as u8);
    }

    #[test]
    fn sequence_number_is_echoed_in_response() {
        let req: &[u8] = &[
            0x00, 0xAB, 0x00, // seq = 0xAB, ReadAttributes
            0x00, 0x00,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let _ = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(buf[1], 0xAB);
    }

    // build_default_response / should_send_default_response

    #[test]
    fn build_default_response_writes_correct_bytes() {
        use crate::header::command_identifier::CommandIdentifier;
        let mut buf = [0u8; 8];
        let n = build_default_response(
            CommandIdentifier::ReadAttributes,
            Status::UnsupportedAttribute,
            0x42,
            &mut buf,
        )
        .unwrap();
        assert_eq!(n, 5);
        assert_eq!(buf[0], 0x18);
        assert_eq!(buf[1], 0x42);
        assert_eq!(buf[2], 0x0b);
        assert_eq!(buf[3], 0x00); // ReadAttributes raw
        assert_eq!(buf[4], Status::UnsupportedAttribute as u8);
    }

    #[test]
    fn build_default_response_for_frame_preserves_manufacturer_code() {
        let req: &[u8] = &[
            0x04, // global | manufacturer-specific | client→server
            0x78, 0x56, // manufacturer code
            0x42, 0x00,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 8];
        let n =
            build_default_response_for_frame(&frame, Status::UnsupportedCluster, &mut buf).unwrap();

        assert_eq!(n, 7);
        assert_eq!(buf[0], 0x1c);
        assert_eq!(&buf[1..3], &[0x78, 0x56]);
        assert_eq!(buf[3], 0x42);
        assert_eq!(buf[4], 0x0b);
        assert_eq!(buf[5], 0x00);
        assert_eq!(buf[6], Status::UnsupportedCluster as u8);
    }
    #[test]
    fn should_send_default_response_suppressed_on_broadcast() {
        let req: &[u8] = &[0x00, 0x01, 0x06];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        assert!(!should_send_default_response(
            &frame,
            broadcast(),
            Status::UnsupCommand
        ));
    }

    #[test]
    fn should_send_default_response_suppressed_for_success_with_disable_bit() {
        // frame_control 0x10 = disable-default-response bit set
        let req: &[u8] = &[0x10, 0x01, 0x06];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        assert!(!should_send_default_response(
            &frame,
            unicast(),
            Status::Success
        ));
    }

    #[test]
    fn should_send_default_response_error_ignores_disable_bit() {
        let req: &[u8] = &[0x10, 0x01, 0x06];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        assert!(should_send_default_response(
            &frame,
            unicast(),
            Status::UnsupCommand
        ));
    }

    #[test]
    fn should_send_default_response_suppressed_for_incoming_default_response() {
        let req: &[u8] = &[
            0x18, // global | server→client | disable default response
            0x01, 0x0b, 0x00, 0x00,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        assert!(!should_send_default_response(
            &frame,
            unicast(),
            Status::UnsupportedCluster
        ));
    }

    #[test]
    fn should_send_default_response_suppressed_for_write_no_response() {
        let req: &[u8] = &[0x00, 0x01, 0x05, 0x00, 0x00, 0x20, 0x01];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        assert!(!should_send_default_response(
            &frame,
            unicast(),
            Status::UnsupportedCluster
        ));
    }

    #[test]
    fn write_no_response_malformed_payload_stops_without_loop() {
        let req: &[u8] = &[
            0x00, 0x30, 0x05, // WriteAttributesNoResponse
            0x01, 0x00, 0xFF, // unknown type id; no value length is recoverable
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn server_to_client_global_write_does_not_mutate_server() {
        let req: &[u8] = &[
            0x08, // global | server→client
            0x31, 0x05, // WriteAttributesNoResponse
            0x01, 0x00, 0x21, 0x34, 0x12,
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 32];
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();
        assert_eq!(n, 0);

        let mut rbuf = [0u8; 2];
        server
            .store
            .read_into(AttributeId::new(0x0001), &mut rbuf)
            .unwrap();
        assert_eq!(rbuf, [0x00, 0x00]);
    }

    #[test]
    fn discover_attributes_caps_records_to_response_buffer() {
        let req: &[u8] = &[
            0x00, 0x32, 0x0c, // DiscoverAttributes
            0x00, 0x00, // start_attr = 0x0000
            0xFF, // max_count = 255
        ];
        let (frame, _) = IncomingZclFrame::decode(req).unwrap();
        let mut buf = [0u8; 7]; // header + complete flag + one 3-byte record
        let mut server = TestServer::new();
        let n = zcl_cluster_dispatch(&mut server, &frame, unicast(), &mut buf).unwrap();

        assert_eq!(n, 7);
        assert_eq!(buf[3], 0x00); // more attributes remain
        assert_eq!(buf[4..6], [0x00, 0x00]);
        assert_eq!(buf[6], TypeId::Uint8.as_u8());
    }
}
