//! Attribute reporting.
//!
//! See ZCL Section 2.5.7 / 2.5.8 / 2.5.11.
//!
//! [`AttributeReportBuilder`] assembles a `Report Attributes` frame from
//! attribute descriptors, so a record carries the identifier and type the
//! descriptor declares and the caller supplies only the value.
//! [`ConfigureReportingServer`] answers `Configure Reporting` (global command
//! 0x06) requests, which a coordinator issues after binding to set up attribute
//! reporting during the post-interview configuration step.

use byte::BytesExt;
use byte::TryRead;
use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::frame::header::ZclHeader;
use crate::frame::header::command_identifier::CommandIdentifier;
use crate::frame::header::frame_control::FrameControl;
use crate::server::RESPONSE_FRAME_CONTROL;
use crate::types::codec::ZclKind;
use crate::types::descriptors::AccessTypestate;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Reportable;
use crate::types::ids::ClusterId;

/// A serialized `Report Attributes` frame and the cluster it reports on.
///
/// Produced by [`AttributeReportBuilder::finish`]. Carrying the cluster
/// alongside the payload means the transport cannot address the report to a
/// cluster the attributes do not belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportFrame<'a> {
    cluster: ClusterId,
    asdu: &'a [u8],
}

impl<'a> ReportFrame<'a> {
    /// Cluster the reported attributes belong to.
    pub const fn cluster_id(&self) -> ClusterId {
        self.cluster
    }

    /// Serialized frame, ready to be sent as an APS payload.
    pub const fn asdu(&self) -> &'a [u8] {
        self.asdu
    }
}

/// Builds a `Report Attributes` frame (ZCL 2.5.11) from attribute descriptors.
///
/// Each record takes its identifier and data type from the descriptor, so the
/// only way to report a value is with the type the specification gives that
/// attribute. Attributes the specification does not mark reportable have no
/// [`Attribute::report`] method and cannot be pushed at all.
///
/// Records are written straight into the caller's buffer; the frame grows
/// until the buffer is full rather than up to a fixed record count.
pub struct AttributeReportBuilder<'a> {
    buf: &'a mut [u8],
    offset: usize,
    cluster: Option<ClusterId>,
}

impl<'a> AttributeReportBuilder<'a> {
    /// Start a frame, writing the ZCL header into `buf`.
    ///
    /// The header is a global command sent server to client with the default
    /// response disabled, the configuration a device reporting to its
    /// coordinator uses (2.4.1.1).
    pub fn new(buf: &'a mut [u8], sequence_number: u8) -> byte::Result<Self> {
        let header = ZclHeader {
            frame_control: FrameControl(RESPONSE_FRAME_CONTROL),
            manufacturer_code: None,
            sequence_number,
            command_identifier: CommandIdentifier::ReportAttributes,
        };

        let offset = &mut 0;
        buf.write_with(offset, header, ())?;
        let offset = *offset;

        Ok(Self {
            buf,
            offset,
            cluster: None,
        })
    }

    /// Append a record for `attribute`.
    ///
    /// Every attribute in one frame must belong to the same cluster, since the
    /// cluster is carried by the enclosing APS frame rather than the record.
    pub fn push<T, A>(
        mut self,
        attribute: &Attribute<T, A, Reportable>,
        value: T::Value<'_>,
    ) -> byte::Result<Self>
    where
        T: ZclKind,
        A: AccessTypestate,
    {
        let cluster = attribute.cluster().id();
        if self.cluster.is_some_and(|existing| existing != cluster) {
            return Err(bad_input!("attribute report mixes clusters"));
        }
        self.cluster = Some(cluster);

        let offset = &mut self.offset;
        attribute.report(value, self.buf, offset)?;
        Ok(self)
    }

    /// Finish the frame.
    ///
    /// Fails when no record was pushed: `Report Attributes` carries at least
    /// one attribute record (2.5.11.1).
    pub fn finish(self) -> byte::Result<ReportFrame<'a>> {
        let Self {
            buf,
            offset,
            cluster,
        } = self;
        let cluster = cluster.ok_or(bad_input!("attribute report has no records"))?;

        Ok(ReportFrame {
            cluster,
            asdu: &buf[..offset],
        })
    }
}

/// Answers ZCL `Configure Reporting` requests with a blanket success.
///
/// This device emits attribute reports on its own schedule rather than from
/// coordinator-configured intervals, so every reporting configuration is
/// accepted with status SUCCESS regardless of cluster. The responder is generic
/// across clusters; pair it with the cluster servers that own the attributes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigureReportingServer;

impl ClusterRequestHandler for ConfigureReportingServer {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        // only the header is needed to ack; the configuration records carry
        // variable-length reportable-change fields we do not act on
        let (header, _) = ZclHeader::try_read(request.asdu, ()).ok()?;
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
        // single SUCCESS status record covers all attributes (ZCL 2.5.8.1.3)
        out.write_with(offset, Status::Success, ()).ok()?;
        Some(ClusterReply::matching(request, *offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(cluster_id: u16, asdu: &[u8]) -> ClusterRequest<'_> {
        ClusterRequest {
            profile_id: 0x0104,
            cluster_id,
            src_endpoint: 1,
            dst_endpoint: 1,
            unicast: true,
            asdu,
        }
    }

    #[test]
    fn acks_configure_reporting_with_success() {
        // Configure Reporting (global cmd 0x06) for temperature MeasuredValue;
        // the trailing configuration record is intentionally ignored
        let asdu = [
            0x00, 0x2b, 0x06, // frame control (global, c->s), seq, ConfigureReporting
            0x00, 0x00, 0x00, // direction, attribute id 0x0000
            0x29, 0x1e, 0x00, 0x58, 0x02, 0x64, 0x00, // type int16, intervals, change
        ];
        let mut out = [0u8; 16];
        let reply = ConfigureReportingServer
            .handle(&request(0x0402, &asdu), &mut out)
            .expect("configure reporting handled");
        // header (0x18, seq 0x2b, ConfigureReportingResponse 0x07) + status
        // SUCCESS
        assert_eq!(&out[..reply.len], &[0x18, 0x2b, 0x07, 0x00]);
    }

    #[test]
    fn ignores_other_commands() {
        // Read Attributes (0x00) is not ours
        let asdu = [0x00, 0x01, 0x00, 0x04, 0x00];
        let mut out = [0u8; 16];
        assert_eq!(
            ConfigureReportingServer.handle(&request(0x0000, &asdu), &mut out),
            None
        );
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;
    use crate::clusters::measurement::temperature;
    use crate::types::descriptors::Cluster;
    use crate::types::integers::Int16;

    // 2.5.11: header followed by one attribute record per reported attribute
    #[test]
    fn builds_a_report_frame_from_descriptors() {
        let mut buf = [0u8; 32];
        let report = AttributeReportBuilder::new(&mut buf, 0x2b)
            .expect("header written")
            .push(&temperature::MEASURED_VALUE, Int16(2300))
            .expect("record written")
            .finish()
            .expect("frame finished");

        assert_eq!(report.cluster_id(), temperature::CLUSTER.id());
        assert_eq!(
            report.asdu(),
            &[
                0x18, 0x2b, 0x0a, // frame control, sequence, ReportAttributes
                0x00, 0x00, 0x29, 0xfc, 0x08, // MeasuredValue, int16, 2300
            ]
        );
    }

    #[test]
    fn a_report_without_records_is_rejected() {
        let mut buf = [0u8; 32];
        assert!(
            AttributeReportBuilder::new(&mut buf, 0x01)
                .expect("header written")
                .finish()
                .is_err()
        );
    }

    // the cluster travels in the APS frame, so one report cannot span clusters
    #[test]
    fn mixing_clusters_in_one_report_is_rejected() {
        let mut buf = [0u8; 32];
        let other = Cluster::new(ClusterId(0x0403), "PressureMeasurement")
            .attribute::<Int16, crate::types::descriptors::ReadOnly, Reportable>(
            crate::types::ids::AttributeId(0x0000),
            "MeasuredValue",
        );

        let result = AttributeReportBuilder::new(&mut buf, 0x01)
            .expect("header written")
            .push(&temperature::MEASURED_VALUE, Int16(2300))
            .expect("first record written")
            .push(&other, Int16(1000));

        assert!(result.is_err());
    }

    // records are bounded by the buffer, not by a fixed record count
    #[test]
    fn a_full_buffer_stops_the_frame() {
        let mut buf = [0u8; 8];
        let result = AttributeReportBuilder::new(&mut buf, 0x01)
            .expect("header written")
            .push(&temperature::MEASURED_VALUE, Int16(2300))
            .expect("first record fits")
            .push(&temperature::MEASURED_VALUE, Int16(2400));

        assert!(result.is_err());
    }
}
