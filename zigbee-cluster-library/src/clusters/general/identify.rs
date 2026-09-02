//! Identify Cluster
//!
//! See ZCL Section 3.5
//!
//! Lets a commissioning tool ask a device to make itself physically
//! identifiable for a number of seconds. [`IdentifyServer`] owns the
//! countdown; the application decides what identifying looks like by polling
//! [`IdentifyServer::is_identifying`].

use core::sync::atomic::AtomicU16;
use core::sync::atomic::Ordering;

use byte::BytesExt;
use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::frame::ZclFrame;
use crate::frame::header::ZclHeader;
use crate::frame::header::command_identifier::CommandIdentifier;
use crate::frame::header::frame_control::FrameControl;
use crate::frame::payload::ZclFramePayload;
use crate::server::ClusterCommand;
use crate::server::ClusterServer;
use crate::server::CommandOutcome;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadWrite;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::ids::CommandId;
use crate::types::ids::TypeId;
use crate::types::integers::Uint16;

/// Cluster descriptor (ZCL 3.5).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0003), "Identify");

/// Cluster identifier (ZCL 3.5), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// `IdentifyTime` (ZCL 3.5.2.2.1), seconds remaining in the identify state.
///
/// Read/write: writing it starts or stops identification just as the Identify
/// command does.
pub const IDENTIFY_TIME: Attribute<Uint16, ReadWrite> =
    CLUSTER.attribute(AttributeId(0x0000), "IdentifyTime");

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptor above, which stays the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `IdentifyTime` (`uint16`), seconds remaining in the identify state.
    pub const IDENTIFY_TIME: u16 = super::IDENTIFY_TIME.id().0;
}

/// Cluster-specific command identifiers (ZCL 3.5.2.3 / 3.5.2.4).
pub mod command {
    /// Received: start identifying for the payload's duration.
    pub const IDENTIFY: u8 = 0x00;
    /// Received: ask whether the device is currently identifying.
    pub const IDENTIFY_QUERY: u8 = 0x01;
    /// Generated: answer to [`IDENTIFY_QUERY`], carrying the time remaining.
    pub const IDENTIFY_QUERY_RESPONSE: u8 = 0x00;
}

// cluster-specific, server->client, default response disabled
const CLUSTER_RESPONSE_FRAME_CONTROL: u8 = 0x19;

/// Identify cluster server.
///
/// The countdown is not automatic: call [`IdentifyServer::tick`] once a
/// second.
#[derive(Debug, Default)]
pub struct IdentifyServer {
    identify_time: AtomicU16,
}

impl IdentifyServer {
    /// A server that starts out not identifying.
    pub const fn new() -> Self {
        Self {
            identify_time: AtomicU16::new(0),
        }
    }

    /// Seconds remaining in the identify state; `0` means not identifying.
    pub fn identify_time(&self) -> u16 {
        self.identify_time.load(Ordering::Relaxed)
    }

    /// Enter (or leave, with `0`) the identify state for `seconds`.
    pub fn set_identify_time(&self, seconds: u16) {
        self.identify_time.store(seconds, Ordering::Relaxed);
    }

    /// Whether the device should currently be making itself identifiable.
    pub fn is_identifying(&self) -> bool {
        self.identify_time() != 0
    }

    /// Advance the countdown by `seconds`, saturating at zero, and report the
    /// time left.
    pub fn tick(&self, seconds: u16) -> u16 {
        let remaining = self.identify_time().saturating_sub(seconds);
        self.set_identify_time(remaining);
        remaining
    }
}

impl ClusterServer for IdentifyServer {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        if id.0 != attribute::IDENTIFY_TIME {
            return Status::UnsupportedAttribute;
        }
        match IDENTIFY_TIME.encode(Uint16(self.identify_time()), out, offset) {
            Ok(()) => Status::Success,
            Err(_) => Status::InsufficientSpace,
        }
    }

    /// `IdentifyTime` is writable (ZCL 3.5.2.2): a write starts or stops the
    /// identification procedure just like the Identify command does.
    fn decode_value(
        &self,
        id: AttributeId,
        type_id: TypeId,
        bytes: &[u8],
        offset: &mut usize,
    ) -> Status {
        if id.0 != attribute::IDENTIFY_TIME {
            return Status::UnsupportedAttribute;
        }
        // the descriptor declares uint16, so a record carrying any other type
        // identifier is rejected before the value is read
        let Ok(Uint16(seconds)) = IDENTIFY_TIME.decode(type_id, bytes, offset) else {
            return Status::InvalidDataType;
        };
        self.set_identify_time(seconds);
        Status::Success
    }

    /// Cluster-specific commands (ZCL 3.5.2.3).
    fn command(&self, command: ClusterCommand<'_>, out: &mut [u8]) -> CommandOutcome {
        match command.id.0 {
            command::IDENTIFY => {
                // payload: IdentifyTime, uint16 (ZCL 3.5.2.3.1.1)
                let Some(seconds) = command
                    .data
                    .get(..2)
                    .and_then(|bytes| bytes.try_into().ok())
                    .map(u16::from_le_bytes)
                else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                self.set_identify_time(seconds);
                CommandOutcome::Status(Status::Success)
            }
            // answered by its own response only while identifying, which draws
            // no Default Response (ZCL 3.5.2.4.1, 2.5.12.2)
            command::IDENTIFY_QUERY if self.is_identifying() => self
                .write_query_response(command.sequence_number, out)
                .map_or(
                    CommandOutcome::Status(Status::InsufficientSpace),
                    CommandOutcome::Response,
                ),
            command::IDENTIFY_QUERY => CommandOutcome::Status(Status::Success),
            _ => CommandOutcome::Status(Status::UnsupCommand),
        }
    }
}

impl IdentifyServer {
    /// Write an `Identify Query Response` carrying the time remaining
    /// (ZCL 3.5.2.4.1).
    fn write_query_response(&self, sequence_number: u8, out: &mut [u8]) -> Option<usize> {
        let remaining = self.identify_time().to_le_bytes();
        let response = ZclFrame {
            header: ZclHeader {
                frame_control: FrameControl(CLUSTER_RESPONSE_FRAME_CONTROL),
                manufacturer_code: None,
                sequence_number,
                command_identifier: CommandIdentifier::from_bits(command::IDENTIFY_QUERY_RESPONSE),
            },
            payload: ZclFramePayload::ClusterSpecificCommand {
                command_id: CommandId(command::IDENTIFY_QUERY_RESPONSE),
                data: &remaining,
            },
        };

        let offset = &mut 0;
        out.write_with(offset, response, ()).ok()?;
        Some(*offset)
    }
}

impl ClusterRequestHandler for IdentifyServer {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::GeneralCommand;
    use crate::types::data_types::UnsignedN;
    use crate::types::data_types::ZclDataType;
    use byte::TryRead;

    // cluster-specific, client->server, Identify for 30 s
    const IDENTIFY_30S: [u8; 5] = [0x01, 0x2a, 0x00, 0x1e, 0x00];
    // same, with the default response disabled
    const IDENTIFY_30S_NO_DEFAULT_RSP: [u8; 5] = [0x11, 0x2a, 0x00, 0x1e, 0x00];
    // cluster-specific, client->server, Identify Query
    const IDENTIFY_QUERY: [u8; 3] = [0x01, 0x2b, 0x01];

    fn request(asdu: &[u8]) -> ClusterRequest<'_> {
        ClusterRequest {
            profile_id: 0x0104,
            cluster_id: CLUSTER_ID,
            src_endpoint: 1,
            dst_endpoint: 1,
            unicast: true,
            asdu,
        }
    }

    #[test]
    fn identify_command_sets_the_countdown() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];

        assert!(!server.is_identifying());
        let reply = server
            .handle(&request(&IDENTIFY_30S), &mut out)
            .expect("default response for a unicast Identify");
        // global s->c frame, seq echoed, DefaultResponse (0x0b) for command
        // 0x00 with status SUCCESS
        assert_eq!(&out[..reply.len], &[0x18, 0x2a, 0x0b, 0x00, 0x00]);
        assert_eq!(server.identify_time(), 30);
        assert!(server.is_identifying());
    }

    #[test]
    fn identify_command_honors_disabled_default_response() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];

        assert_eq!(
            server.handle(&request(&IDENTIFY_30S_NO_DEFAULT_RSP), &mut out),
            None
        );
        assert_eq!(server.identify_time(), 30);

        // a broadcast never draws a default response either (ZCL 2.5.12.2)
        let broadcast = ClusterRequest {
            unicast: false,
            ..request(&IDENTIFY_30S)
        };
        assert_eq!(server.handle(&broadcast, &mut out), None);
    }

    #[test]
    fn identify_query_answers_only_while_identifying() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];

        // not identifying: no query response, only a default response
        let reply = server
            .handle(&request(&IDENTIFY_QUERY), &mut out)
            .expect("default response while not identifying");
        assert_eq!(&out[..reply.len], &[0x18, 0x2b, 0x0b, 0x01, 0x00]);

        server.set_identify_time(30);
        let reply = server
            .handle(&request(&IDENTIFY_QUERY), &mut out)
            .expect("query answered while identifying");
        // frame control 0x19 (cluster-specific, s->c, no default response),
        // seq echoed, command 0x00, then IdentifyTime as uint16
        assert_eq!(&out[..reply.len], &[0x19, 0x2b, 0x00, 0x1e, 0x00]);
    }

    #[test]
    fn unsupported_command_reports_unsup_command() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];
        // Trigger Effect (0x40) is optional and not implemented
        let asdu = [0x01, 0x2c, 0x40, 0x00, 0x00];
        let reply = server
            .handle(&request(&asdu), &mut out)
            .expect("default response for an unknown command");
        assert_eq!(&out[..reply.len], &[0x18, 0x2c, 0x0b, 0x40, 0x81]);
    }

    #[test]
    fn writes_identify_time_attribute() {
        let server = IdentifyServer::new();
        // global Write Attributes: IdentifyTime (0x0000), uint16 (0x21), 30 s
        let asdu = [0x00, 0x0a, 0x02, 0x00, 0x00, 0x21, 0x1e, 0x00];
        let mut out = [0u8; 32];
        let reply = server
            .handle(&request(&asdu), &mut out)
            .expect("attribute write handled");
        // header + single SUCCESS record (ZCL 2.5.4.1.2)
        assert_eq!(&out[..reply.len], &[0x18, 0x0a, 0x04, 0x00]);
        assert_eq!(server.identify_time(), 30);
    }

    #[test]
    fn tick_counts_down_and_saturates() {
        let server = IdentifyServer::new();
        server.set_identify_time(3);
        assert_eq!(server.tick(1), 2);
        assert_eq!(server.tick(5), 0);
        assert!(!server.is_identifying());
        // already at zero, stays there
        assert_eq!(server.tick(1), 0);
    }

    #[test]
    fn reads_identify_time_attribute() {
        let server = IdentifyServer::new();
        server.set_identify_time(0x1234);
        // global Read Attributes for IdentifyTime (0x0000)
        let asdu = [0x00, 0x07, 0x00, 0x00, 0x00];
        let mut out = [0u8; 32];
        let reply = server
            .handle(&request(&asdu), &mut out)
            .expect("attribute read handled");

        let (frame, _) = ZclFrame::try_read(&out[..reply.len], ()).unwrap();
        let ZclFramePayload::GeneralCommand(GeneralCommand::ReadAttributesResponse(records)) =
            frame.payload
        else {
            panic!("expected ReadAttributesResponse");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].attribute_id, attribute::IDENTIFY_TIME);
        assert_eq!(records[0].status, Status::Success);
        assert_eq!(
            records[0].value,
            Some(ZclDataType::UnsignedInt(UnsignedN::Uint16(0x1234)))
        );
    }

    #[test]
    fn ignores_other_clusters() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];
        let other = ClusterRequest {
            cluster_id: 0x0402,
            ..request(&IDENTIFY_30S)
        };
        assert_eq!(server.handle(&other, &mut out), None);
    }
}
