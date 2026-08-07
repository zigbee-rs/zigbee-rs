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
use byte::TryRead;
use zigbee::zdo::ClusterRequestHandler;

use crate::attributes::AttributeSource;
use crate::attributes::handle_read_attributes;
use crate::common::data_types::UnsignedN;
use crate::common::data_types::ZclDataType;
use crate::frame::ZclFrame;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameControl;
use crate::payload::ZclFramePayload;
use crate::types::ids::CommandId;

/// Cluster identifier (ZCL 3.5).
pub const CLUSTER_ID: u16 = 0x0003;

/// Attribute identifiers (ZCL 3.5.2.2).
pub mod attribute {
    /// `IdentifyTime` (`uint16`), seconds remaining in the identify state.
    pub const IDENTIFY_TIME: u16 = 0x0000;
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

impl AttributeSource for IdentifyServer {
    fn cluster_id(&self) -> u16 {
        CLUSTER_ID
    }

    fn attribute(&self, id: u16) -> Option<ZclDataType<'_>> {
        match id {
            attribute::IDENTIFY_TIME => Some(ZclDataType::UnsignedInt(UnsignedN::Uint16(
                self.identify_time(),
            ))),
            _ => None,
        }
    }
}

impl ClusterRequestHandler for IdentifyServer {
    fn handle(
        &self,
        _profile_id: u16,
        cluster_id: u16,
        _src_endpoint: u8,
        _dst_endpoint: u8,
        asdu: &[u8],
        out: &mut [u8],
    ) -> Option<usize> {
        if cluster_id != CLUSTER_ID {
            return None;
        }

        let (frame, _) = ZclFrame::try_read(asdu, ()).ok()?;
        let ZclFramePayload::ClusterSpecificCommand { command_id, data } = frame.payload else {
            return handle_read_attributes(self, cluster_id, asdu, out);
        };

        match command_id.0 {
            command::IDENTIFY => {
                // payload: IdentifyTime, uint16 (ZCL 3.5.2.3.1.1)
                let seconds = u16::from_le_bytes(data.get(..2)?.try_into().ok()?);
                self.set_identify_time(seconds);
                None
            }
            // answered only while identifying (ZCL 3.5.2.4.1)
            command::IDENTIFY_QUERY if self.is_identifying() => {
                let remaining = self.identify_time().to_le_bytes();
                let response = ZclFrame {
                    header: ZclHeader {
                        frame_control: FrameControl(CLUSTER_RESPONSE_FRAME_CONTROL),
                        manufacturer_code: None,
                        sequence_number: frame.header.sequence_number,
                        command_identifier: CommandIdentifier::from_bits(
                            command::IDENTIFY_QUERY_RESPONSE,
                        ),
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
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::GeneralCommand;
    use crate::frame::Status;

    // cluster-specific, client->server, Identify for 30 s
    const IDENTIFY_30S: [u8; 5] = [0x01, 0x2a, 0x00, 0x1e, 0x00];
    // cluster-specific, client->server, Identify Query
    const IDENTIFY_QUERY: [u8; 3] = [0x01, 0x2b, 0x01];

    #[test]
    fn identify_command_sets_the_countdown() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];

        assert!(!server.is_identifying());
        // the command produces no reply, only state
        assert_eq!(
            server.handle(0x0104, CLUSTER_ID, 1, 1, &IDENTIFY_30S, &mut out),
            None
        );
        assert_eq!(server.identify_time(), 30);
        assert!(server.is_identifying());
    }

    #[test]
    fn identify_query_answers_only_while_identifying() {
        let server = IdentifyServer::new();
        let mut out = [0u8; 32];

        // not identifying: silence
        assert_eq!(
            server.handle(0x0104, CLUSTER_ID, 1, 1, &IDENTIFY_QUERY, &mut out),
            None
        );

        server.set_identify_time(30);
        let n = server
            .handle(0x0104, CLUSTER_ID, 1, 1, &IDENTIFY_QUERY, &mut out)
            .expect("query answered while identifying");
        // frame control 0x19 (cluster-specific, s->c, no default response),
        // seq echoed, command 0x00, then IdentifyTime as uint16
        assert_eq!(&out[..n], &[0x19, 0x2b, 0x00, 0x1e, 0x00]);
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
        let request = [0x00, 0x07, 0x00, 0x00, 0x00];
        let mut out = [0u8; 32];
        let n = server
            .handle(0x0104, CLUSTER_ID, 1, 1, &request, &mut out)
            .expect("attribute read handled");

        let (frame, _) = ZclFrame::try_read(&out[..n], ()).unwrap();
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
        assert_eq!(
            server.handle(0x0104, 0x0402, 1, 1, &IDENTIFY_30S, &mut out),
            None
        );
    }
}
