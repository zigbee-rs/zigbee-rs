//! General ZCL Frame
#![allow(missing_docs)]

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::ctx;
use heapless::Vec;
use zigbee_macros::impl_byte;

use crate::cluster_server::DispatchContext;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameType;
use crate::header::manufacturer_code::ManufacturerCode;
use crate::payload::WriteAttributesPayload;
use crate::types::descriptors::AttrInfo;
use crate::types::error::ZclError;
use crate::types::ids::AttributeId;
use crate::types::ids::CommandId;
use crate::types::ids::RawTypeId;
use crate::types::value::ZclValueRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    ClientToServer,
    ServerToClient,
}

impl Direction {
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::ClientToServer => Self::ServerToClient,
            Self::ServerToClient => Self::ClientToServer,
        }
    }

    pub const fn from_wire_bit(bit: bool) -> Self {
        if bit {
            Self::ServerToClient
        } else {
            Self::ClientToServer
        }
    }

    pub const fn wire_bit(self) -> bool {
        matches!(self, Self::ServerToClient)
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawCommandId(u8);

impl RawCommandId {
    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn known_global(self) -> Option<CommandIdentifier> {
        match CommandIdentifier::from_bits(self.0) {
            CommandIdentifier::Reserved(_) => None,
            known => Some(known),
        }
    }

    pub const fn cluster_specific(self) -> CommandId {
        CommandId::new(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZclFrameMeta {
    pub manufacturer_code: Option<ManufacturerCode>,
    pub sequence_number: u8,
    pub direction: Direction,
    pub disable_default_response: bool,
}

impl ZclFrameMeta {
    pub const fn new(sequence_number: u8, direction: Direction) -> Self {
        Self {
            manufacturer_code: None,
            sequence_number,
            direction,
            disable_default_response: false,
        }
    }

    #[must_use]
    pub const fn with_manufacturer_code(mut self, code: ManufacturerCode) -> Self {
        self.manufacturer_code = Some(code);
        self
    }

    #[must_use]
    pub const fn disable_default_response(mut self) -> Self {
        self.disable_default_response = true;
        self
    }

    #[must_use]
    pub const fn enable_default_response(mut self) -> Self {
        self.disable_default_response = false;
        self
    }

    pub const fn response_to(request: &IncomingZclFrame<'_>) -> Self {
        Self {
            manufacturer_code: request.manufacturer_code(),
            sequence_number: request.sequence_number(),
            direction: request.direction().opposite(),
            disable_default_response: true,
        }
    }

    fn from_header(header: ZclHeader) -> Self {
        Self {
            manufacturer_code: header.manufacturer_code,
            sequence_number: header.sequence_number,
            direction: Direction::from_wire_bit(header.frame_control.direction()),
            disable_default_response: header.frame_control.disable_default_response(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ZclFrameParts<C> {
    meta: ZclFrameMeta,
    command: C,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IncomingZclFrame<'a> {
    parts: ZclFrameParts<IncomingZclCommand<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutgoingZclFrame<'a> {
    parts: ZclFrameParts<OutgoingZclCommand<'a>>,
}

pub type ReadAttributes = Vec<ReadAttribute, 16>;

#[derive(Clone, Debug, PartialEq)]
pub enum IncomingZclCommand<'a> {
    Global(IncomingGlobalCommand<'a>),
    ClusterSpecific {
        command_id: CommandId,
        data: &'a [u8],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum IncomingGlobalCommand<'a> {
    ReadAttributes(ReadAttributes),
    WriteAttributes(WriteAttributesPayload<'a>),
    WriteAttributesUndivided(WriteAttributesPayload<'a>),
    WriteAttributesNoResponse(WriteAttributesPayload<'a>),
    DiscoverAttributes {
        start_attr: AttributeId,
        max_count: u8,
    },
    DiscoverCommandsReceived {
        start_cmd: u8,
        max_count: u8,
    },
    DiscoverCommandsGenerated {
        start_cmd: u8,
        max_count: u8,
    },
    DiscoverAttributesExtended {
        start_attr: AttributeId,
        max_count: u8,
    },
    DefaultResponse(DefaultResponse),
    KnownUnhandled {
        command_id: CommandIdentifier,
        data: &'a [u8],
    },
    Unknown {
        command_id: RawCommandId,
        data: &'a [u8],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutgoingZclCommand<'a> {
    Global(OutgoingGlobalCommand<'a>),
    ClusterSpecific {
        command_id: CommandId,
        data: &'a [u8],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutgoingGlobalCommand<'a> {
    ReadAttributesResponse(&'a [ReadAttributeResponse<'a>]),
    WriteAttributesResponse(&'a [WriteAttributeStatus]),
    ReportAttributes(&'a [AttributeReport<'a>]),
    DefaultResponse(DefaultResponse),
    DiscoverAttributesResponse(DiscoverAttributesResponse<'a>),
    UnknownRaw {
        command_id: RawCommandId,
        data: &'a [u8],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoverAttributesResponse<'a> {
    pub discovery_complete: bool,
    pub attributes: &'a [AttrInfo],
}

impl<'a> IncomingZclFrame<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<(Self, usize), ZclError> {
        let (header, header_len) = ZclHeader::try_read(bytes, ()).map_err(ZclError::from)?;
        let meta = ZclFrameMeta::from_header(header);
        let payload = &bytes[header_len..];

        let (command, payload_len) = match header.frame_control.frame_type() {
            FrameType::GlobalCommand => decode_incoming_global(header.command_identifier, payload)?,
            FrameType::ClusterCommand => (
                IncomingZclCommand::ClusterSpecific {
                    command_id: CommandId::new(header.command_identifier.raw()),
                    data: payload,
                },
                payload.len(),
            ),
            FrameType::Reserved => return Err(ZclError::InvalidValue),
        };

        Ok((
            Self {
                parts: ZclFrameParts { meta, command },
            },
            header_len + payload_len,
        ))
    }

    pub const fn meta(&self) -> ZclFrameMeta {
        self.parts.meta
    }

    pub const fn command(&self) -> &IncomingZclCommand<'a> {
        &self.parts.command
    }

    pub const fn manufacturer_code(&self) -> Option<ManufacturerCode> {
        self.parts.meta.manufacturer_code
    }

    pub const fn sequence_number(&self) -> u8 {
        self.parts.meta.sequence_number
    }

    pub const fn direction(&self) -> Direction {
        self.parts.meta.direction
    }

    pub const fn disable_default_response(&self) -> bool {
        self.parts.meta.disable_default_response
    }

    pub const fn is_global(&self) -> bool {
        matches!(self.parts.command, IncomingZclCommand::Global(_))
    }

    pub const fn is_cluster_specific(&self) -> bool {
        matches!(
            self.parts.command,
            IncomingZclCommand::ClusterSpecific { .. }
        )
    }

    pub const fn command_id(&self) -> RawCommandId {
        match &self.parts.command {
            IncomingZclCommand::Global(command) => RawCommandId::new(command.raw_id()),
            IncomingZclCommand::ClusterSpecific { command_id, .. } => {
                RawCommandId::new(command_id.0)
            }
        }
    }

    pub const fn global_command_id(&self) -> Option<CommandIdentifier> {
        match &self.parts.command {
            IncomingZclCommand::Global(command) => command.command_identifier(),
            IncomingZclCommand::ClusterSpecific { .. } => None,
        }
    }

    pub const fn cluster_command_id(&self) -> Option<CommandId> {
        match &self.parts.command {
            IncomingZclCommand::Global(_) => None,
            IncomingZclCommand::ClusterSpecific { command_id, .. } => Some(*command_id),
        }
    }

    pub fn should_send_default_response(&self, ctx: DispatchContext, status: Status) -> bool {
        crate::cluster_server::should_send_default_response(self, ctx, status)
    }

    pub fn default_response(&self, status: Status) -> Option<OutgoingZclFrame<'static>> {
        OutgoingZclFrame::default_response(self, status)
    }
}

#[doc(hidden)]
impl<'a> TryRead<'a, ()> for IncomingZclFrame<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        Self::decode(bytes).map_err(ZclError::into)
    }
}

impl IncomingGlobalCommand<'_> {
    const fn raw_id(&self) -> u8 {
        match self {
            Self::ReadAttributes(_) => 0x00,
            Self::WriteAttributes(_) => 0x02,
            Self::WriteAttributesUndivided(_) => 0x03,
            Self::WriteAttributesNoResponse(_) => 0x05,
            Self::DefaultResponse(_) => 0x0b,
            Self::DiscoverAttributes { .. } => 0x0c,
            Self::DiscoverCommandsReceived { .. } => 0x11,
            Self::DiscoverCommandsGenerated { .. } => 0x13,
            Self::DiscoverAttributesExtended { .. } => 0x15,
            Self::KnownUnhandled { command_id, .. } => command_id.raw(),
            Self::Unknown { command_id, .. } => command_id.raw(),
        }
    }

    const fn command_identifier(&self) -> Option<CommandIdentifier> {
        match self {
            Self::ReadAttributes(_) => Some(CommandIdentifier::ReadAttributes),
            Self::WriteAttributes(_) => Some(CommandIdentifier::WriteAttributes),
            Self::WriteAttributesUndivided(_) => Some(CommandIdentifier::WriteAttributesUndivided),
            Self::WriteAttributesNoResponse(_) => {
                Some(CommandIdentifier::WriteAttributesNoResponse)
            }
            Self::DefaultResponse(_) => Some(CommandIdentifier::DefaultResponse),
            Self::DiscoverAttributes { .. } => Some(CommandIdentifier::DiscoverAttributes),
            Self::DiscoverCommandsReceived { .. } => {
                Some(CommandIdentifier::DiscoverCommandsReceived)
            }
            Self::DiscoverCommandsGenerated { .. } => {
                Some(CommandIdentifier::DiscoverCommandsGenerated)
            }
            Self::DiscoverAttributesExtended { .. } => {
                Some(CommandIdentifier::DiscoverAttributesExtended)
            }
            Self::KnownUnhandled { command_id, .. } => Some(*command_id),
            Self::Unknown { .. } => None,
        }
    }
}

impl<'a> OutgoingZclFrame<'a> {
    pub const fn new(meta: ZclFrameMeta, command: OutgoingZclCommand<'a>) -> Self {
        Self {
            parts: ZclFrameParts { meta, command },
        }
    }

    pub const fn global(meta: ZclFrameMeta, command: OutgoingGlobalCommand<'a>) -> Self {
        Self::new(meta, OutgoingZclCommand::Global(command))
    }

    pub const fn cluster_specific(
        meta: ZclFrameMeta,
        command_id: CommandId,
        data: &'a [u8],
    ) -> Self {
        Self::new(
            meta,
            OutgoingZclCommand::ClusterSpecific { command_id, data },
        )
    }

    pub fn reply_to(request: &IncomingZclFrame<'_>, command: OutgoingZclCommand<'a>) -> Self {
        Self::new(ZclFrameMeta::response_to(request), command)
    }

    pub fn default_response(request: &IncomingZclFrame<'_>, status: Status) -> Option<Self> {
        if matches!(
            request.command(),
            IncomingZclCommand::Global(
                IncomingGlobalCommand::DefaultResponse(_)
                    | IncomingGlobalCommand::WriteAttributesNoResponse(_)
            )
        ) {
            return None;
        }

        Some(Self::reply_to(
            request,
            OutgoingZclCommand::Global(OutgoingGlobalCommand::DefaultResponse(DefaultResponse {
                command_identifier: request.command_id().raw(),
                status,
            })),
        ))
    }

    pub const fn meta(&self) -> ZclFrameMeta {
        self.parts.meta
    }

    pub const fn command(&self) -> &OutgoingZclCommand<'a> {
        &self.parts.command
    }

    pub const fn manufacturer_code(&self) -> Option<ManufacturerCode> {
        self.parts.meta.manufacturer_code
    }

    pub const fn sequence_number(&self) -> u8 {
        self.parts.meta.sequence_number
    }

    pub const fn direction(&self) -> Direction {
        self.parts.meta.direction
    }

    pub const fn disable_default_response(&self) -> bool {
        self.parts.meta.disable_default_response
    }

    pub const fn is_global(&self) -> bool {
        matches!(self.parts.command, OutgoingZclCommand::Global(_))
    }

    pub const fn is_cluster_specific(&self) -> bool {
        matches!(
            self.parts.command,
            OutgoingZclCommand::ClusterSpecific { .. }
        )
    }

    pub const fn command_id(&self) -> RawCommandId {
        RawCommandId::new(self.parts.command.command_byte())
    }

    pub fn encoded_len(&self) -> usize {
        frame_header_len(self.manufacturer_code()) + self.parts.command.payload_len()
    }

    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, ZclError> {
        let header_len = write_frame_header(
            buf,
            self.parts.command.frame_type(),
            self.meta(),
            self.parts.command.command_byte(),
        )?;
        let payload_len = self.parts.command.write_payload(&mut buf[header_len..])?;
        Ok(header_len + payload_len)
    }
}

impl OutgoingZclCommand<'_> {
    const fn frame_type(&self) -> FrameType {
        match self {
            Self::Global(_) => FrameType::GlobalCommand,
            Self::ClusterSpecific { .. } => FrameType::ClusterCommand,
        }
    }

    const fn command_byte(&self) -> u8 {
        match self {
            Self::Global(command) => command.command_byte(),
            Self::ClusterSpecific { command_id, .. } => command_id.0,
        }
    }

    fn payload_len(&self) -> usize {
        match self {
            Self::Global(command) => command.payload_len(),
            Self::ClusterSpecific { data, .. } => data.len(),
        }
    }

    fn write_payload(&self, buf: &mut [u8]) -> Result<usize, ZclError> {
        match self {
            Self::Global(command) => command.write_payload(buf),
            Self::ClusterSpecific { data, .. } => copy_payload(buf, data),
        }
    }
}

impl OutgoingGlobalCommand<'_> {
    const fn command_byte(&self) -> u8 {
        match self {
            Self::ReadAttributesResponse(_) => 0x01,
            Self::WriteAttributesResponse(_) => 0x04,
            Self::ReportAttributes(_) => 0x0a,
            Self::DefaultResponse(_) => 0x0b,
            Self::DiscoverAttributesResponse(_) => 0x0d,
            Self::UnknownRaw { command_id, .. } => command_id.raw(),
        }
    }

    fn payload_len(&self) -> usize {
        match self {
            Self::ReadAttributesResponse(records) => {
                records.iter().map(read_attribute_response_len).sum()
            }
            Self::WriteAttributesResponse(records) => write_attributes_response_len(records),
            Self::ReportAttributes(records) => records.iter().map(attribute_report_len).sum(),
            Self::DefaultResponse(_) => 2,
            Self::DiscoverAttributesResponse(response) => 1 + response.attributes.len() * 3,
            Self::UnknownRaw { data, .. } => data.len(),
        }
    }

    fn write_payload(&self, buf: &mut [u8]) -> Result<usize, ZclError> {
        let offset = &mut 0;
        match self {
            Self::ReadAttributesResponse(records) => {
                for record in *records {
                    write_read_attribute_response(buf, offset, record)?;
                }
            }
            Self::WriteAttributesResponse(records) => {
                write_normalized_attribute_statuses(buf, offset, records)?;
            }
            Self::ReportAttributes(records) => {
                for record in *records {
                    write_attribute_report(buf, offset, record)?;
                }
            }
            Self::DefaultResponse(response) => {
                write_byte_payload(buf, offset, response.command_identifier)?;
                write_status_payload(buf, offset, response.status)?;
            }
            Self::DiscoverAttributesResponse(response) => {
                write_byte_payload(buf, offset, u8::from(response.discovery_complete))?;
                for attr in response.attributes {
                    write_u16_payload(buf, offset, attr.id.0)?;
                    write_byte_payload(buf, offset, attr.type_id.as_u8())?;
                }
            }
            Self::UnknownRaw { data, .. } => {
                *offset = copy_payload(buf, data)?;
            }
        }
        Ok(*offset)
    }
}

fn decode_incoming_global(
    command_identifier: CommandIdentifier,
    bytes: &[u8],
) -> Result<(IncomingZclCommand<'_>, usize), ZclError> {
    let offset = &mut 0;
    let global = match command_identifier {
        CommandIdentifier::ReadAttributes => {
            let mut attrs = Vec::new();
            while *offset < bytes.len() {
                let attr = bytes.read_with(offset, ()).map_err(ZclError::from)?;
                attrs.push(attr).map_err(|_| ZclError::BufferTooSmall)?;
            }
            IncomingGlobalCommand::ReadAttributes(attrs)
        }
        CommandIdentifier::WriteAttributes => {
            *offset = bytes.len();
            IncomingGlobalCommand::WriteAttributes(WriteAttributesPayload(bytes))
        }
        CommandIdentifier::WriteAttributesUndivided => {
            *offset = bytes.len();
            IncomingGlobalCommand::WriteAttributesUndivided(WriteAttributesPayload(bytes))
        }
        CommandIdentifier::WriteAttributesNoResponse => {
            *offset = bytes.len();
            IncomingGlobalCommand::WriteAttributesNoResponse(WriteAttributesPayload(bytes))
        }
        CommandIdentifier::DiscoverAttributes => {
            if bytes.len() < 3 {
                return Err(ZclError::InsufficientBytes);
            }
            *offset = 3;
            IncomingGlobalCommand::DiscoverAttributes {
                start_attr: AttributeId::new(u16::from_le_bytes([bytes[0], bytes[1]])),
                max_count: bytes[2],
            }
        }
        CommandIdentifier::DiscoverCommandsReceived => {
            if bytes.len() < 2 {
                return Err(ZclError::InsufficientBytes);
            }
            *offset = 2;
            IncomingGlobalCommand::DiscoverCommandsReceived {
                start_cmd: bytes[0],
                max_count: bytes[1],
            }
        }
        CommandIdentifier::DiscoverCommandsGenerated => {
            if bytes.len() < 2 {
                return Err(ZclError::InsufficientBytes);
            }
            *offset = 2;
            IncomingGlobalCommand::DiscoverCommandsGenerated {
                start_cmd: bytes[0],
                max_count: bytes[1],
            }
        }
        CommandIdentifier::DiscoverAttributesExtended => {
            if bytes.len() < 3 {
                return Err(ZclError::InsufficientBytes);
            }
            *offset = 3;
            IncomingGlobalCommand::DiscoverAttributesExtended {
                start_attr: AttributeId::new(u16::from_le_bytes([bytes[0], bytes[1]])),
                max_count: bytes[2],
            }
        }
        CommandIdentifier::DefaultResponse => IncomingGlobalCommand::DefaultResponse(
            bytes.read_with(offset, ()).map_err(ZclError::from)?,
        ),
        CommandIdentifier::Reserved(raw) => {
            *offset = bytes.len();
            IncomingGlobalCommand::Unknown {
                command_id: RawCommandId::new(raw),
                data: bytes,
            }
        }
        known => {
            *offset = bytes.len();
            IncomingGlobalCommand::KnownUnhandled {
                command_id: known,
                data: bytes,
            }
        }
    };

    Ok((IncomingZclCommand::Global(global), *offset))
}

fn frame_header_len(manufacturer_code: Option<ManufacturerCode>) -> usize {
    3 + if manufacturer_code.is_some() { 2 } else { 0 }
}

fn write_frame_header(
    buf: &mut [u8],
    frame_type: FrameType,
    meta: ZclFrameMeta,
    command_id: u8,
) -> Result<usize, ZclError> {
    let needed = frame_header_len(meta.manufacturer_code);
    if buf.len() < needed {
        return Err(ZclError::BufferTooSmall);
    }

    let frame_type_bits = match frame_type {
        FrameType::GlobalCommand => 0x00,
        FrameType::ClusterCommand => 0x01,
        FrameType::Reserved => return Err(ZclError::InvalidValue),
    };
    let manufacturer_bit = if meta.manufacturer_code.is_some() {
        0x04
    } else {
        0x00
    };
    let direction_bit = if meta.direction.wire_bit() {
        0x08
    } else {
        0x00
    };
    let disable_default_response_bit = if meta.disable_default_response {
        0x10
    } else {
        0x00
    };

    let mut n = 0;
    buf[n] = frame_type_bits | manufacturer_bit | direction_bit | disable_default_response_bit;
    n += 1;
    if let Some(manufacturer_code) = meta.manufacturer_code {
        buf[n..n + 2].copy_from_slice(&manufacturer_code.0.to_le_bytes());
        n += 2;
    }
    buf[n] = meta.sequence_number;
    n += 1;
    buf[n] = command_id;
    n += 1;
    Ok(n)
}

fn copy_payload(buf: &mut [u8], data: &[u8]) -> Result<usize, ZclError> {
    buf.get_mut(..data.len())
        .ok_or(ZclError::BufferTooSmall)?
        .copy_from_slice(data);
    Ok(data.len())
}

fn write_byte_payload(buf: &mut [u8], offset: &mut usize, value: u8) -> Result<(), ZclError> {
    *buf.get_mut(*offset).ok_or(ZclError::BufferTooSmall)? = value;
    *offset += 1;
    Ok(())
}

fn write_status_payload(
    buf: &mut [u8],
    offset: &mut usize,
    status: Status,
) -> Result<(), ZclError> {
    let written = status.encode(buf.get_mut(*offset..).ok_or(ZclError::BufferTooSmall)?)?;
    *offset += written;
    Ok(())
}

fn write_u16_payload(buf: &mut [u8], offset: &mut usize, value: u16) -> Result<(), ZclError> {
    buf.get_mut(*offset..*offset + 2)
        .ok_or(ZclError::BufferTooSmall)?
        .copy_from_slice(&value.to_le_bytes());
    *offset += 2;
    Ok(())
}

fn write_zcl_value_ref(
    buf: &mut [u8],
    offset: &mut usize,
    value: &ZclValueRef<'_>,
) -> Result<(), ZclError> {
    write_byte_payload(buf, offset, value.type_id().as_u8())?;
    let written = value.encode(buf.get_mut(*offset..).ok_or(ZclError::BufferTooSmall)?)?;
    *offset += written;
    Ok(())
}

fn write_read_attribute_response(
    buf: &mut [u8],
    offset: &mut usize,
    record: &ReadAttributeResponse<'_>,
) -> Result<(), ZclError> {
    write_u16_payload(buf, offset, record.attribute_id)?;
    write_status_payload(buf, offset, record.status)?;
    match (record.status, &record.value) {
        (Status::Success, Some(value)) => write_zcl_value_ref(buf, offset, value),
        (Status::Success, None) | (_, Some(_)) => Err(ZclError::InvalidValue),
        (_, None) => Ok(()),
    }
}

fn write_attribute_report(
    buf: &mut [u8],
    offset: &mut usize,
    record: &AttributeReport<'_>,
) -> Result<(), ZclError> {
    write_u16_payload(buf, offset, record.attribute_id)?;
    write_zcl_value_ref(buf, offset, &record.value)
}

fn write_normalized_attribute_statuses(
    buf: &mut [u8],
    offset: &mut usize,
    records: &[WriteAttributeStatus],
) -> Result<(), ZclError> {
    let has_failures = records.iter().any(|r| r.status != Status::Success);
    if has_failures {
        for record in records {
            if record.status != Status::Success {
                write_status_payload(buf, offset, record.status)?;
                write_u16_payload(
                    buf,
                    offset,
                    record.attribute_id.ok_or(ZclError::InvalidValue)?,
                )?;
            }
        }
    } else {
        write_status_payload(buf, offset, Status::Success)?;
    }
    Ok(())
}

fn read_attribute_response_len(record: &ReadAttributeResponse<'_>) -> usize {
    3 + match (record.status, &record.value) {
        (Status::Success, Some(value)) => 1 + value.encoded_len(),
        _ => 0,
    }
}

fn attribute_report_len(record: &AttributeReport<'_>) -> usize {
    3 + record.value.encoded_len()
}

fn write_attributes_response_len(records: &[WriteAttributeStatus]) -> usize {
    if records.iter().any(|r| r.status != Status::Success) {
        records
            .iter()
            .filter(|r| r.status != Status::Success)
            .count()
            * 3
    } else {
        1
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
    /// The specified cluster-specific command is not supported on the device.
    /// Command not carried out.
    UnsupCommand = 0x81,
    /// ~The specified global (general) ZCL command is not supported on the
    /// device. Command not carried out.~
    ///
    /// Use [`Status::UnsupCommand`]
    #[deprecated = "Use `Status::UnsupCommand`"]
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
    /// Decode a `Status` from the first byte of `data`.
    /// Returns `(status, bytes_consumed)` or a `ZclError`.
    pub fn decode(data: &[u8]) -> Result<(Self, usize), ZclError> {
        use byte::TryRead as _;
        Self::try_read(data, ()).map_err(ZclError::from)
    }

    /// Encode this `Status` into `buf`. Returns bytes written or a `ZclError`.
    pub fn encode(self, buf: &mut [u8]) -> Result<usize, ZclError> {
        use byte::TryWrite as _;
        self.try_write(buf, ()).map_err(ZclError::from)
    }

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

#[doc(hidden)]
impl<'a> TryRead<'a, ()> for Status {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let raw: u8 = bytes.read_with(offset, ctx::LE)?;
        // Errata: deprecated wire bytes are substituted for their replacements.
        let status = match raw {
            0x82..=0x84 => Self::UnsupCommand,
            0x8a | 0xc4 => Self::Success,
            0x8f => Self::NotAuthorized,
            0x90 | 0x91 | 0x93 | 0xc0 | 0xc1 => Self::Failure,
            other => Self::from_byte(other)?,
        };
        Ok((status, *offset))
    }
}

#[doc(hidden)]
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

impl_byte! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ReadAttribute {
        pub attribute_id: u16,
    }
}

#[derive(Debug, PartialEq)]
pub struct ReadAttributeResponse<'a> {
    pub attribute_id: u16,
    pub status: Status,
    pub value: Option<ZclValueRef<'a>>,
}

#[doc(hidden)]
impl<'a> TryRead<'a, ()> for ReadAttributeResponse<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let attribute_id = bytes.read_with(offset, ctx::LE)?;
        let status: Status = bytes.read_with(offset, ())?;
        let value = if status == Status::Success {
            let type_byte: u8 = bytes.read_with(offset, ctx::LE)?;
            let type_id = RawTypeId::new(type_byte)
                .known()
                .ok_or(bad_input!("unknown ZCL type id"))?;
            let remaining = bytes
                .get(*offset..)
                .ok_or(bad_input!("insufficient bytes"))?;
            let (val, consumed) = ZclValueRef::decode_with_type(type_id, remaining)
                .map_err(|_| bad_input!("ZCL value decode error"))?;
            *offset += consumed;
            Some(val)
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

#[doc(hidden)]
impl TryWrite<()> for ReadAttributeResponse<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.attribute_id, ctx::LE)?;
        bytes.write_with(offset, self.status, ())?;
        if self.status == Status::Success {
            let value = self.value.ok_or(bad_input!(
                "successful ReadAttributeResponse requires value"
            ))?;
            bytes.write_with(offset, value.type_id().as_u8(), ctx::LE)?;
            let n = value
                .encode(
                    bytes
                        .get_mut(*offset..)
                        .ok_or(bad_input!("buffer too small"))?,
                )
                .map_err(|_| bad_input!("ZCL value encode error"))?;
            *offset += n;
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
    pub value: ZclValueRef<'a>,
}

#[doc(hidden)]
impl<'a> TryRead<'a, ()> for WriteAttribute<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let attribute_id = bytes.read_with(offset, ctx::LE)?;
        let type_byte: u8 = bytes.read_with(offset, ctx::LE)?;
        let type_id = RawTypeId::new(type_byte)
            .known()
            .ok_or(bad_input!("unknown ZCL type id"))?;
        let remaining = bytes
            .get(*offset..)
            .ok_or(bad_input!("insufficient bytes"))?;
        let (value, consumed) = ZclValueRef::decode_with_type(type_id, remaining)
            .map_err(|_| bad_input!("ZCL value decode error"))?;
        *offset += consumed;

        Ok((
            Self {
                attribute_id,
                value,
            },
            *offset,
        ))
    }
}

#[doc(hidden)]
impl TryWrite<()> for WriteAttribute<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.attribute_id, ctx::LE)?;
        bytes.write_with(offset, self.value.type_id().as_u8(), ctx::LE)?;
        let n = self
            .value
            .encode(
                bytes
                    .get_mut(*offset..)
                    .ok_or(bad_input!("buffer too small"))?,
            )
            .map_err(|_| bad_input!("ZCL value encode error"))?;
        *offset += n;
        Ok(*offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteAttributeStatus {
    pub status: Status,
    pub attribute_id: Option<u16>,
}

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
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
    pub value: ZclValueRef<'a>,
}

#[doc(hidden)]
impl<'a> TryRead<'a, ()> for AttributeReport<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let attribute_id: u16 = bytes.read_with(offset, ctx::LE)?;
        let type_byte: u8 = bytes.read_with(offset, ctx::LE)?;
        let type_id = RawTypeId::new(type_byte)
            .known()
            .ok_or(bad_input!("unknown ZCL type id"))?;
        let remaining = bytes
            .get(*offset..)
            .ok_or(bad_input!("insufficient bytes"))?;
        let (value, consumed) = ZclValueRef::decode_with_type(type_id, remaining)
            .map_err(|_| bad_input!("ZCL value decode error"))?;
        *offset += consumed;

        Ok((
            Self {
                attribute_id,
                value,
            },
            *offset,
        ))
    }
}

#[doc(hidden)]
impl TryWrite<()> for AttributeReport<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.attribute_id, ctx::LE)?;
        bytes.write_with(offset, self.value.type_id().as_u8(), ctx::LE)?;
        let n = self
            .value
            .encode(
                bytes
                    .get_mut(*offset..)
                    .ok_or(bad_input!("buffer too small"))?,
            )
            .map_err(|_| bad_input!("ZCL value encode error"))?;
        *offset += n;
        Ok(*offset)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use byte::TryRead;
    use byte::TryWrite;

    use super::*;
    use crate::types::ids::TypeId;
    use crate::types::strings::ZclText;

    #[test]
    fn direction_opposite_maps_both_directions() {
        assert_eq!(
            Direction::ClientToServer.opposite(),
            Direction::ServerToClient
        );
        assert_eq!(
            Direction::ServerToClient.opposite(),
            Direction::ClientToServer
        );
    }

    #[test]
    fn raw_command_id_interprets_global_and_cluster_contexts() {
        let raw = RawCommandId::new(0x0b);
        assert_eq!(raw.raw(), 0x0b);
        assert_eq!(raw.known_global(), Some(CommandIdentifier::DefaultResponse));
        assert_eq!(raw.cluster_specific(), CommandId::new(0x0b));
        assert_eq!(RawCommandId::new(0x80).known_global(), None);
        for raw in 0u8..=u8::MAX {
            assert_eq!(RawCommandId::new(raw).raw(), raw);
            assert_eq!(
                RawCommandId::new(raw).cluster_specific(),
                CommandId::new(raw)
            );
        }
    }

    #[test]
    fn response_meta_preserves_sequence_and_manufacturer_and_flips_direction() {
        let input = [0x04, 0x34, 0x12, 0x55, 0x00];
        let (frame, _) = IncomingZclFrame::decode(&input).expect("incoming frame parses");

        let meta = ZclFrameMeta::response_to(&frame);

        assert_eq!(meta.manufacturer_code, Some(ManufacturerCode(0x1234)));
        assert_eq!(meta.sequence_number, 0x55);
        assert_eq!(meta.direction, Direction::ServerToClient);
        assert!(meta.disable_default_response);
    }

    #[test]
    fn parse_attribute_report_payload() {
        let input: &[u8] = &[
            0x00, 0x00, // identifier
            0x29, 0xab, 0x03,
        ];

        let (report, _) = AttributeReport::try_read(input, ())
            .expect("Failed to read AttributeReport payload in test");

        assert_eq!(report.attribute_id, 0u16);
        assert_eq!(report.value, ZclValueRef::Int16(939));
    }

    #[test]
    fn incoming_report_attributes_is_known_unhandled_global() {
        let input: &[u8] = &[
            0x18, // frame control
            0x01, // sequence number
            0x0A, // command identifier
            0x00, 0x00, 0x29, 0x3f, 0x0a, // payload
        ];

        let (frame, used) = IncomingZclFrame::decode(input).expect("Failed to read incoming frame");

        assert_eq!(used, input.len());
        assert_eq!(frame.sequence_number(), 0x01);
        assert_eq!(frame.direction(), Direction::ServerToClient);
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::KnownUnhandled {
                command_id: CommandIdentifier::ReportAttributes,
                data
            }) if *data == &input[3..]
        ));
    }

    #[test]
    fn incoming_global_request_variants_and_unknown_decode() {
        let (frame, _) =
            IncomingZclFrame::decode(&[0x00, 0x10, 0x03, 0x01, 0x00, 0x20, 0x2a]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributesUndivided(_))
        ));

        let (frame, _) =
            IncomingZclFrame::decode(&[0x00, 0x11, 0x05, 0x01, 0x00, 0x20, 0x2a]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributesNoResponse(_))
        ));

        let (frame, _) = IncomingZclFrame::decode(&[0x00, 0x12, 0x0c, 0x34, 0x12, 0x7f]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::DiscoverAttributes {
                start_attr: AttributeId(0x1234),
                max_count: 0x7f,
            })
        ));

        let (frame, _) = IncomingZclFrame::decode(&[0x00, 0x13, 0x80, 0xde, 0xad]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::Unknown {
                command_id,
                data,
            }) if *command_id == RawCommandId::new(0x80) && *data == [0xde, 0xad]
        ));
    }

    #[test]
    fn incoming_discover_commands_received_decodes_start_and_max() {
        // 0x11 = DiscoverCommandsReceived, payload: start_cmd=0x03, max_count=0x10
        let (frame, _) = IncomingZclFrame::decode(&[0x00, 0x14, 0x11, 0x03, 0x10]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::DiscoverCommandsReceived {
                start_cmd: 0x03,
                max_count: 0x10,
            })
        ));
        assert_eq!(frame.command_id().raw(), 0x11);
    }

    #[test]
    fn incoming_discover_commands_generated_decodes_start_and_max() {
        // 0x13 = DiscoverCommandsGenerated
        let (frame, _) = IncomingZclFrame::decode(&[0x00, 0x15, 0x13, 0x00, 0xFF]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::DiscoverCommandsGenerated {
                start_cmd: 0x00,
                max_count: 0xFF,
            })
        ));
        assert_eq!(frame.command_id().raw(), 0x13);
    }

    #[test]
    fn incoming_discover_attributes_extended_decodes_start_and_max() {
        // 0x15 = DiscoverAttributesExtended, payload: start_attr=0x0010, max_count=0x08
        let (frame, _) = IncomingZclFrame::decode(&[0x00, 0x16, 0x15, 0x10, 0x00, 0x08]).unwrap();
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::DiscoverAttributesExtended {
                start_attr: AttributeId(0x0010),
                max_count: 0x08,
            })
        ));
        assert_eq!(frame.command_id().raw(), 0x15);
    }

    #[test]
    fn incoming_discover_commands_too_short_is_error() {
        assert!(IncomingZclFrame::decode(&[0x00, 0x01, 0x11, 0x00]).is_err());
        assert!(IncomingZclFrame::decode(&[0x00, 0x02, 0x13]).is_err());
        assert!(IncomingZclFrame::decode(&[0x00, 0x03, 0x15, 0x00]).is_err());
    }

    #[test]
    fn incoming_cluster_specific_command_preserves_command_id_once() {
        let input: &[u8] = &[
            0x19, // frame control
            0x01, // sequence number
            0x80, // command identifier
            0x00, 0x00, 0x29, 0x3f, 0x0a, // payload
        ];

        let (frame, used) = IncomingZclFrame::decode(input).expect("Failed to read incoming frame");

        assert_eq!(used, input.len());
        assert_eq!(frame.command_id().raw(), 0x80);
        assert_eq!(frame.cluster_command_id(), Some(CommandId::new(0x80)));
        assert_eq!(frame.global_command_id(), None);
        assert!(matches!(
            frame.command(),
            IncomingZclCommand::ClusterSpecific { command_id, data }
                if *command_id == CommandId::new(0x80) && *data == &input[3..]
        ));
    }

    #[test]
    fn incoming_read_attributes_decodes_records() {
        let input: &[u8] = &[
            0x00, // frame control: global, client to server
            0x11, // sequence number
            0x00, // Read Attributes
            0x00, 0x00, // ZCLVersion
            0x04, 0x00, // ManufacturerName
        ];

        let (frame, len) = IncomingZclFrame::decode(input).expect("read attributes request parses");
        assert_eq!(len, input.len());

        let IncomingZclCommand::Global(IncomingGlobalCommand::ReadAttributes(attrs)) =
            frame.command()
        else {
            panic!("ReadAttributesCommand expected");
        };
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].attribute_id, 0x0000);
        assert_eq!(attrs[1].attribute_id, 0x0004);
    }

    #[test]
    fn incoming_write_attributes_decodes_raw_payload() {
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

        let (frame, len) =
            IncomingZclFrame::decode(input).expect("write attributes request parses");
        assert_eq!(len, input.len());

        let IncomingZclCommand::Global(IncomingGlobalCommand::WriteAttributes(payload)) =
            frame.command()
        else {
            panic!("WriteAttributes expected");
        };
        let mut iter = payload.records();
        let r0 = iter.next().expect("record 0").expect("record 0 ok");
        let r1 = iter.next().expect("record 1").expect("record 1 ok");
        assert!(iter.next().is_none());

        assert_eq!(r0.attr_id, AttributeId::new(0x0001));
        assert_eq!(r0.type_id, TypeId::Boolean);
        assert_eq!(r0.value, &[0x01u8]);
        assert_eq!(r1.attr_id, AttributeId::new(0x0002));
        assert_eq!(r1.type_id, TypeId::CharacterString);
        assert_eq!(r1.value, &[0x02, b'O', b'K']);
    }

    #[test]
    fn outgoing_read_attributes_response_writes_spec_records() {
        let records = [
            ReadAttributeResponse {
                attribute_id: 0x0000, // ZCLVersion
                status: Status::Success,
                value: Some(ZclValueRef::Uint8(8)),
            },
            ReadAttributeResponse {
                attribute_id: 0x0007, // PowerSource
                status: Status::Success,
                value: Some(ZclValueRef::Enum8(0x03)),
            },
            ReadAttributeResponse {
                attribute_id: 0x0004, // ManufacturerName
                status: Status::Success,
                value: Some(ZclValueRef::ShortText(Some(ZclText::new(b"Acme")))),
            },
        ];
        let frame = OutgoingZclFrame::global(
            ZclFrameMeta::new(0x22, Direction::ServerToClient).disable_default_response(),
            OutgoingGlobalCommand::ReadAttributesResponse(&records),
        );

        let expected: &[u8] = &[
            0x18, 0x22, 0x01, // header
            0x00, 0x00, 0x00, 0x20, 0x08, // ZCLVersion: success, uint8, 8
            0x07, 0x00, 0x00, 0x30, 0x03, // PowerSource: success, enum8, battery
            0x04, 0x00, 0x00, 0x42, 0x04, b'A', b'c', b'm', b'e',
        ];

        let mut output = [0u8; 32];
        let written = frame
            .encode(&mut output)
            .expect("basic cluster read response writes");
        assert_eq!(written, expected.len());
        assert_eq!(frame.encoded_len(), expected.len());
        assert_eq!(&output[..written], expected);

        let (parsed, parsed_len) =
            IncomingZclFrame::decode(&output[..written]).expect("response frame parses");
        assert_eq!(parsed_len, expected.len());
        assert_eq!(parsed.sequence_number(), 0x22);
        assert!(matches!(
            parsed.command(),
            IncomingZclCommand::Global(IncomingGlobalCommand::KnownUnhandled {
                command_id: CommandIdentifier::ReadAttributesResponse,
                ..
            })
        ));
    }

    #[test]
    fn outgoing_write_attributes_response_normalizes_failures_and_success() {
        let mixed = [
            WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            },
            WriteAttributeStatus {
                status: Status::UnsupportedAttribute,
                attribute_id: Some(0x0001),
            },
        ];
        let frame = OutgoingZclFrame::global(
            ZclFrameMeta::new(0x20, Direction::ServerToClient).disable_default_response(),
            OutgoingGlobalCommand::WriteAttributesResponse(&mixed),
        );

        let mut buf = [0u8; 16];
        let written = frame.encode(&mut buf).expect("normalizes mixed on write");
        let expected: &[u8] = &[
            0x18, 0x20, 0x04, // header
            0x86, 0x01, 0x00, // UNSUPPORTED_ATTRIBUTE for attribute 0x0001
        ];
        assert_eq!(&buf[..written], expected);

        let successes = [
            WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            },
            WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            },
        ];
        let frame = OutgoingZclFrame::global(
            ZclFrameMeta::new(0x21, Direction::ServerToClient).disable_default_response(),
            OutgoingGlobalCommand::WriteAttributesResponse(&successes),
        );
        let written = frame.encode(&mut buf).expect("normalizes multiple success");
        assert_eq!(&buf[..written], &[0x18, 0x21, 0x04, 0x00]);
    }

    #[test]
    fn incoming_default_response_parses_and_outgoing_default_response_encodes() {
        let input: &[u8] = &[
            0x18, // frame control: global, server to client, default response disabled
            0x14, // sequence number
            0x0b, // Default Response
            0x00, // command identifier being responded to: Read Attributes
            0x00, // success
        ];

        let (frame, len) = IncomingZclFrame::decode(input).expect("default response parses");
        assert_eq!(len, input.len());

        let IncomingZclCommand::Global(IncomingGlobalCommand::DefaultResponse(dr)) =
            frame.command()
        else {
            panic!("DefaultResponse expected");
        };
        assert_eq!(dr.command_identifier, 0x00);
        assert_eq!(dr.status, Status::Success);

        assert!(OutgoingZclFrame::default_response(&frame, Status::Success).is_none());

        let request = IncomingZclFrame::decode(&[0x00, 0x14, 0x00]).unwrap().0;
        let response = OutgoingZclFrame::default_response(&request, Status::UnsupportedAttribute)
            .expect("default response should be created for request");
        let mut output = [0u8; 8];
        let written = response
            .encode(&mut output)
            .expect("default response writes");
        assert_eq!(&output[..written], &[0x18, 0x14, 0x0b, 0x00, 0x86]);
    }

    #[test]
    fn incoming_decode_rejects_bad_required_payloads() {
        assert!(IncomingZclFrame::decode(&[0x18, 0x14, 0x0b, 0x02, 0x7d]).is_err());
        assert!(IncomingZclFrame::decode(&[0x00, 0x01, 0x0c, 0x00]).is_err());
        assert!(IncomingZclFrame::decode(&[0x02, 0x01, 0x00]).is_err());
        assert!(IncomingZclFrame::decode(&[0x04, 0x01]).is_err());
    }

    #[test]
    fn outgoing_cluster_specific_encodes_from_single_command_id_source() {
        let frame = OutgoingZclFrame::cluster_specific(
            ZclFrameMeta::new(0x33, Direction::ClientToServer),
            CommandId::new(0x80),
            &[0xde, 0xad],
        );

        let mut output = [0u8; 8];
        let written = frame.encode(&mut output).expect("cluster command writes");

        assert_eq!(written, 5);
        assert_eq!(&output[..written], &[0x01, 0x33, 0x80, 0xde, 0xad]);
    }

    #[test]
    fn outgoing_manufacturer_specific_global_encodes_header_bit_and_code() {
        let frame = OutgoingZclFrame::global(
            ZclFrameMeta::new(0x44, Direction::ServerToClient)
                .with_manufacturer_code(ManufacturerCode(0x1234))
                .disable_default_response(),
            OutgoingGlobalCommand::DefaultResponse(DefaultResponse {
                command_identifier: 0x00,
                status: Status::Success,
            }),
        );

        let mut output = [0u8; 8];
        let written = frame
            .encode(&mut output)
            .expect("manufacturer frame writes");

        assert_eq!(written, 7);
        assert_eq!(
            &output[..written],
            &[0x1c, 0x34, 0x12, 0x44, 0x0b, 0x00, 0x00]
        );
    }

    #[test]
    fn outgoing_too_small_buffer_returns_zcl_error() {
        let frame = OutgoingZclFrame::cluster_specific(
            ZclFrameMeta::new(0x33, Direction::ClientToServer),
            CommandId::new(0x80),
            &[0xde, 0xad],
        );

        assert_eq!(frame.encode(&mut [0u8; 2]), Err(ZclError::BufferTooSmall));
    }

    #[test]
    fn read_attribute_response_rejects_inconsistent_status_value_combinations() {
        let mut buf = [0u8; 16];

        let no_value = ReadAttributeResponse {
            attribute_id: 0x0000,
            status: Status::Success,
            value: None,
        };
        assert!(no_value.try_write(&mut buf, ()).is_err());

        let spurious_value = ReadAttributeResponse {
            attribute_id: 0x0000,
            status: Status::UnsupportedAttribute,
            value: Some(ZclValueRef::Uint8(0)),
        };
        assert!(spurious_value.try_write(&mut buf, ()).is_err());
    }

    // Status::TryRead
    #[test]
    fn status_try_read_canonical_bytes() {
        let cases: &[(u8, Status)] = &[
            (0x00, Status::Success),
            (0x01, Status::Failure),
            (0x7e, Status::NotAuthorized),
            (0x7f, Status::Reserved),
            (0x80, Status::MalformedCommand),
            (0x81, Status::UnsupCommand),
            (0x82, Status::UnsupCommand),
            (0x83, Status::UnsupCommand),
            (0x84, Status::UnsupCommand),
            (0x85, Status::InvalidField),
            (0x86, Status::UnsupportedAttribute),
            (0x87, Status::InvalidValue),
            (0x88, Status::ReadOnly),
            (0x89, Status::InsufficientSpace),
            (0x8b, Status::NotFound),
            (0x8c, Status::UnreportableAttribute),
            (0x8d, Status::InvalidDataType),
            (0x8e, Status::InvalidSelector),
            (0x92, Status::ReservedInconsistent),
            (0x94, Status::Timeout),
            (0x95, Status::Abort),
            (0x96, Status::InvalidImage),
            (0x97, Status::WaitForData),
            (0x98, Status::NoImageAvailable),
            (0x99, Status::RequireMoreImage),
            (0x9a, Status::NotificationPending),
            (0xc2, Status::ReservedCalibration),
            (0xc3, Status::UnsupportedCluster),
            // 0x8a (DuplicateExists) and 0xc4 (LimitReached) substitute to Success.
            (0x8a, Status::Success),
            (0xc4, Status::Success),
            // 0x8f (WriteOnly) substitutes to NotAuthorized.
            (0x8f, Status::NotAuthorized),
            // 0x90 (InconsistentStartupState), 0x91 (DefinedOutOfBand), 0x93 (ActionDenied),
            // 0xc0 (HardwareFailure), 0xc1 (SoftwareFailure) all substitute to Failure.
            (0x90, Status::Failure),
            (0x91, Status::Failure),
            (0x93, Status::Failure),
            (0xc0, Status::Failure),
            (0xc1, Status::Failure),
        ];
        for &(byte, expected) in cases {
            let (status, n) = Status::try_read(&[byte], ())
                .unwrap_or_else(|_| panic!("byte 0x{byte:02x} should parse"));
            assert_eq!(status, expected, "byte 0x{byte:02x}");
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn status_try_read_unknown_bytes_are_errors() {
        for byte in [0x02u8, 0x7d, 0x9b, 0xbf, 0xfe, 0xff] {
            assert!(
                Status::try_read(&[byte], ()).is_err(),
                "byte 0x{byte:02x} should be an error"
            );
        }
    }

    #[test]
    fn status_try_read_empty_slice_is_error() {
        assert!(Status::try_read(&[], ()).is_err());
    }

    // Status::TryWrite
    #[allow(deprecated)]
    #[test]
    fn status_try_write_encodes_expected_byte() {
        let cases: &[(Status, u8)] = &[
            // non-deprecated variants encode as their discriminant
            (Status::Success, 0x00),
            (Status::Failure, 0x01),
            (Status::NotAuthorized, 0x7e),
            (Status::MalformedCommand, 0x80),
            (Status::UnsupCommand, 0x81),
            (Status::UnsupportedAttribute, 0x86),
            (Status::Timeout, 0x94),
            (Status::UnsupportedCluster, 0xc3),
            (Status::UnsupGeneralCommand, 0x81),
            // deprecated -> UnsupCommand (0x81)
            (Status::UnsupManufClusterCommand, 0x81),
            (Status::UnsupManufGeneralCommand, 0x81),
            // deprecated -> Success (0x00)
            (Status::DuplicateExists, 0x00),
            (Status::LimitReached, 0x00),
            // deprecated -> NotAuthorized (0x7e)
            (Status::WriteOnly, 0x7e),
            // deprecated -> Failure (0x01)
            (Status::InconsistentStartupState, 0x01),
            (Status::DefinedOutOfBand, 0x01),
            (Status::ActionDenied, 0x01),
            (Status::HardwareFailure, 0x01),
            (Status::SoftwareFailure, 0x01),
        ];
        let mut buf = [0u8; 2];
        for &(variant, expected_byte) in cases {
            let n = variant
                .try_write(&mut buf, ())
                .unwrap_or_else(|_| panic!("{variant:?} should write"));
            assert_eq!(n, 1);
            assert_eq!(buf[0], expected_byte, "{variant:?}");
        }
    }

    #[test]
    fn status_try_write_unknown_is_error() {
        let mut buf = [0u8; 2];
        assert!(Status::Unknown.try_write(&mut buf, ()).is_err());
    }

    // WriteAttribute TryRead/TryWrite
    #[test]
    fn write_attribute_roundtrips() {
        let input: &[u8] = &[
            0x01, 0x00, // attribute id
            0x10, // boolean type
            0x01, // true
        ];

        let (attr, n) = WriteAttribute::try_read(input, ()).expect("WriteAttribute parses");
        assert_eq!(n, input.len());
        assert_eq!(attr.attribute_id, 0x0001);
        assert_eq!(attr.value, ZclValueRef::Bool(true));

        let mut buf = [0u8; 8];
        let written = attr.try_write(&mut buf, ()).expect("WriteAttribute writes");
        assert_eq!(written, input.len());
        assert_eq!(&buf[..written], input);
    }

    // WriteAttributeStatus TryWrite error branches

    #[test]
    fn write_attribute_status_success_with_attribute_id_is_error() {
        let mut buf = [0u8; 8];
        let record = WriteAttributeStatus {
            status: Status::Success,
            attribute_id: Some(0x0001),
        };
        assert!(record.try_write(&mut buf, ()).is_err());
    }

    #[test]
    fn write_attribute_status_failure_without_attribute_id_is_error() {
        let mut buf = [0u8; 8];
        let record = WriteAttributeStatus {
            status: Status::UnsupportedAttribute,
            attribute_id: None,
        };
        assert!(record.try_write(&mut buf, ()).is_err());
    }

    // AttributeReport TryRead/TryWrite
    #[test]
    fn attribute_report_roundtrips() {
        let input: &[u8] = &[
            0x00, 0x00, // attribute id
            0x29, 0xab, 0x03, // Int16 = 939
        ];

        let (report, n) = AttributeReport::try_read(input, ()).expect("AttributeReport parses");
        assert_eq!(n, input.len());
        assert_eq!(report.attribute_id, 0x0000);
        assert_eq!(report.value, ZclValueRef::Int16(939));

        let mut buf = [0u8; 8];
        let written = report
            .try_write(&mut buf, ())
            .expect("AttributeReport writes");
        assert_eq!(written, input.len());
        assert_eq!(&buf[..written], input);
    }

    // DefaultResponse TryRead/TryWrite
    #[test]
    fn default_response_direct_roundtrips() {
        let input: &[u8] = &[0x02, 0x86]; // command_id=2, UnsupportedAttribute

        let (dr, n) = DefaultResponse::try_read(input, ()).expect("DefaultResponse parses");
        assert_eq!(n, 2);
        assert_eq!(dr.command_identifier, 0x02);
        assert_eq!(dr.status, Status::UnsupportedAttribute);

        let mut buf = [0u8; 4];
        let written = dr.try_write(&mut buf, ()).expect("DefaultResponse writes");
        assert_eq!(written, 2);
        assert_eq!(&buf[..written], input);
    }

    #[test]
    fn outgoing_default_response_rejects_unknown_status() {
        let frame = OutgoingZclFrame::global(
            ZclFrameMeta::new(0x44, Direction::ServerToClient),
            OutgoingGlobalCommand::DefaultResponse(DefaultResponse {
                command_identifier: 0x00,
                status: Status::Unknown,
            }),
        );
        let mut buf = [0u8; 8];
        assert_eq!(frame.encode(&mut buf), Err(ZclError::InvalidValue));
    }
    #[test]
    fn default_response_rejects_unknown_status_byte_on_read() {
        let input: &[u8] = &[0x00, 0x02]; // command_id=0, 0x02 is unknown
        assert!(DefaultResponse::try_read(input, ()).is_err());
    }
}
