//! Attribute reporting configuration responder.
//!
//! See ZCL Section 2.5.7 / 2.5.8.
//!
//! [`ConfigureReportingServer`] answers `Configure Reporting` (global command
//! 0x06) requests, which a coordinator issues after binding to set up attribute
//! reporting during the post-interview configuration step.

use byte::BytesExt;
use byte::TryRead;
use zigbee::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameControl;

/// Frame control for a global, server→client response with the default
/// response disabled.
const RESPONSE_FRAME_CONTROL: u8 = 0x18;

/// Answers ZCL `Configure Reporting` requests with a blanket success.
///
/// This device emits attribute reports on its own schedule rather than from
/// coordinator-configured intervals, so every reporting configuration is
/// accepted with status SUCCESS regardless of cluster. The responder is generic
/// across clusters; pair it with the cluster servers that own the attributes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigureReportingServer;

impl ClusterRequestHandler for ConfigureReportingServer {
    fn handle(
        &self,
        _profile_id: u16,
        _cluster_id: u16,
        _src_endpoint: u8,
        _dst_endpoint: u8,
        asdu: &[u8],
        out: &mut [u8],
    ) -> Option<usize> {
        // only the header is needed to ack; the configuration records carry
        // variable-length reportable-change fields we do not act on
        let (header, _) = ZclHeader::try_read(asdu, ()).ok()?;
        if header.command_identifier != CommandIdentifier::ConfigureReporting {
            return None;
        }

        let response = ZclHeader {
            frame_control: FrameControl(RESPONSE_FRAME_CONTROL),
            manufacturer_code: None,
            sequence_number: header.sequence_number,
            command_identifier: CommandIdentifier::ConfigureReportingResponse,
        };

        let offset = &mut 0;
        out.write_with(offset, response, ()).ok()?;
        // successful configuration of all attributes collapses to a single
        // status record carrying just SUCCESS (§2.5.8.1.3)
        out.write_with(offset, Status::Success, ()).ok()?;
        Some(*offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acks_configure_reporting_with_success() {
        // Configure Reporting (global cmd 0x06) for temperature MeasuredValue;
        // the trailing configuration record is intentionally ignored.
        let request = [
            0x00, 0x2b, 0x06, // frame control (global, c->s), seq, ConfigureReporting
            0x00, 0x00, 0x00, // direction, attribute id 0x0000
            0x29, 0x1e, 0x00, 0x58, 0x02, 0x64, 0x00, // type int16, intervals, change
        ];
        let mut out = [0u8; 16];
        let n = ConfigureReportingServer
            .handle(0x0104, 0x0402, 1, 1, &request, &mut out)
            .expect("configure reporting handled");
        // header (0x18, seq 0x2b, ConfigureReportingResponse 0x07) + status SUCCESS
        assert_eq!(&out[..n], &[0x18, 0x2b, 0x07, 0x00]);
    }

    #[test]
    fn ignores_other_commands() {
        // Read Attributes (0x00) is not ours.
        let request = [0x00, 0x01, 0x00, 0x04, 0x00];
        let mut out = [0u8; 16];
        assert_eq!(
            ConfigureReportingServer.handle(0x0104, 0x0000, 1, 1, &request, &mut out),
            None
        );
    }
}
