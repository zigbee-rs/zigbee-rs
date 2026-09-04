//! Attribute reporting.
//!
//! See ZCL Section 2.5.7 through 2.5.11.
//!
//! [`AttributeReportBuilder`] assembles a `Report Attributes` frame from
//! attribute descriptors, so a record carries the identifier and type the
//! descriptor declares and the caller supplies only the value.
//!
//! [`ReportingTable`] holds what a coordinator asked for with
//! `Configure Reporting` (2.5.7). A cluster server hands it to
//! [`ClusterServer::reporting`](crate::server::ClusterServer::reporting) so
//! requests are answered from it, and the application asks the same table
//! [`should_report`](AttributeReporting::should_report) before emitting a
//! report — so the configuration that was accepted is the one that drives the
//! reports.

use byte::BytesExt;
use heapless::Vec;
use spin::Mutex;

use crate::frame::header::ZclHeader;
use crate::frame::header::command_identifier::CommandIdentifier;
use crate::frame::header::frame_control::FrameControl;
use crate::server::RESPONSE_FRAME_CONTROL;
use crate::types::codec::ZclKind;
use crate::types::descriptors::AccessTypestate;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
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

/// Terminates a reporting configuration when used as the maximum reporting
/// interval (2.5.7.1.6).
pub const MAX_INTERVAL_DISABLED: u16 = 0xffff;

/// Asks for the default reporting configuration when used as the minimum
/// reporting interval together with a zero maximum (2.5.7.1.6).
pub const MIN_INTERVAL_DEFAULT: u16 = 0xffff;

/// The reporting configuration of one attribute (2.5.7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportingConfig {
    /// Shortest interval between two reports, in seconds; `0` imposes no
    /// minimum (2.5.7.1.5).
    pub min_interval: u16,
    /// Longest interval between two reports, in seconds; `0` turns periodic
    /// reporting off and leaves change-based reporting on (2.5.7.1.6).
    pub max_interval: u16,
    /// Smallest change that triggers a report, as a magnitude — the sign of
    /// the field on the wire is ignored (2.5.7.1.7).
    ///
    /// `0` reports every change, which is also how a discrete attribute
    /// behaves; the field is absent from a discrete attribute's record.
    pub reportable_change: u64,
}

/// Where the reporting configurations a coordinator sets up are kept.
///
/// Split from [`ReportingTable`] so a cluster server can name a store without
/// naming its capacity, and so an application with its own persistence can
/// substitute one. `Sync`, because the receive path fills it and the
/// application reads it.
pub trait AttributeReporting: Sync {
    /// Store `config` for an attribute, replacing any configuration already
    /// held for it. `false` when there is no room left, which the requester
    /// is owed as `INSUFFICIENT_SPACE`.
    fn configure(
        &self,
        cluster: ClusterId,
        attribute: AttributeId,
        config: ReportingConfig,
    ) -> bool;

    /// Forget an attribute's configuration, which is what a maximum reporting
    /// interval of `0xffff` asks for (2.5.7.1.6).
    fn clear(&self, cluster: ClusterId, attribute: AttributeId);

    /// Configuration in force for an attribute, if one was accepted.
    fn configuration(&self, cluster: ClusterId, attribute: AttributeId) -> Option<ReportingConfig>;

    /// Whether `value` is due to be reported at `now_secs` (2.5.11.2).
    ///
    /// `false` for an attribute with no configuration: an unconfigured
    /// attribute reports on whatever default policy the application has, not
    /// on this table's.
    fn should_report(
        &self,
        cluster: ClusterId,
        attribute: AttributeId,
        now_secs: u32,
        value: i64,
    ) -> bool;

    /// Record that `value` was reported at `now_secs`, which is what the next
    /// [`should_report`](Self::should_report) measures against.
    fn reported(&self, cluster: ClusterId, attribute: AttributeId, now_secs: u32, value: i64);
}

#[derive(Clone, Copy)]
struct Entry {
    cluster: ClusterId,
    attribute: AttributeId,
    config: ReportingConfig,
    /// When and at what value this attribute was last reported; `None` until
    /// the first report, whose timing the specification leaves open (2.5.11.2.1).
    last: Option<(u32, i64)>,
}

impl Entry {
    fn matches(&self, cluster: ClusterId, attribute: AttributeId) -> bool {
        self.cluster == cluster && self.attribute == attribute
    }

    fn due(&self, now_secs: u32, value: i64) -> bool {
        if self.config.max_interval == MAX_INTERVAL_DISABLED {
            return false;
        }
        let Some((reported_at, reported_value)) = self.last else {
            return true;
        };

        let elapsed = now_secs.saturating_sub(reported_at);
        // no further report during the minimum interval, whichever rule below
        // would otherwise fire (2.5.11.2.2, 2.5.11.2.3)
        if elapsed < u32::from(self.config.min_interval) {
            return false;
        }
        if self.config.max_interval != 0 && elapsed >= u32::from(self.config.max_interval) {
            return true;
        }

        let change = value.abs_diff(reported_value);
        change != 0 && change >= self.config.reportable_change
    }
}

/// A fixed-capacity table of reporting configurations, holding up to `N`
/// attributes.
///
/// Shared by reference between the receive path, which fills it from
/// `Configure Reporting` requests, and the application, which asks it when to
/// report — so it takes `&self` throughout.
pub struct ReportingTable<const N: usize> {
    entries: Mutex<Vec<Entry, N>>,
}

impl<const N: usize> ReportingTable<N> {
    /// An empty table.
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Number of attributes currently configured.
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Whether no attribute is configured for reporting.
    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

impl<const N: usize> Default for ReportingTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> AttributeReporting for ReportingTable<N> {
    fn configure(
        &self,
        cluster: ClusterId,
        attribute: AttributeId,
        config: ReportingConfig,
    ) -> bool {
        let mut entries = self.entries.lock();
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.matches(cluster, attribute))
        {
            entry.config = config;
            // a fresh configuration measures its change from the value at the
            // time it was set (2.5.11.2.3)
            entry.last = None;
            return true;
        }

        entries
            .push(Entry {
                cluster,
                attribute,
                config,
                last: None,
            })
            .is_ok()
    }

    fn clear(&self, cluster: ClusterId, attribute: AttributeId) {
        let mut entries = self.entries.lock();
        if let Some(index) = entries
            .iter()
            .position(|entry| entry.matches(cluster, attribute))
        {
            entries.swap_remove(index);
        }
    }

    fn configuration(&self, cluster: ClusterId, attribute: AttributeId) -> Option<ReportingConfig> {
        self.entries
            .lock()
            .iter()
            .find(|entry| entry.matches(cluster, attribute))
            .map(|entry| entry.config)
    }

    fn should_report(
        &self,
        cluster: ClusterId,
        attribute: AttributeId,
        now_secs: u32,
        value: i64,
    ) -> bool {
        self.entries
            .lock()
            .iter()
            .find(|entry| entry.matches(cluster, attribute))
            .is_some_and(|entry| entry.due(now_secs, value))
    }

    fn reported(&self, cluster: ClusterId, attribute: AttributeId, now_secs: u32, value: i64) {
        if let Some(entry) = self
            .entries
            .lock()
            .iter_mut()
            .find(|entry| entry.matches(cluster, attribute))
        {
            entry.last = Some((now_secs, value));
        }
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;
    use crate::clusters::measurement::temperature;
    use crate::types::descriptors::Cluster;
    use crate::types::descriptors::ReadOnly;
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
            .attribute::<Int16, ReadOnly, Reportable>(AttributeId(0x0000), "MeasuredValue");

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

#[cfg(test)]
mod table_tests {
    use super::*;

    const CLUSTER: ClusterId = ClusterId(0x0402);
    const MEASURED_VALUE: AttributeId = AttributeId(0x0000);

    fn table() -> ReportingTable<2> {
        ReportingTable::new()
    }

    // 2.5.11.2.1: a report is due once the maximum reporting interval elapsed
    // 2.5.11.2.3: and once the value moved by the reportable change
    #[test]
    fn the_stored_intervals_decide_when_a_report_is_due() {
        let table = table();
        assert!(table.configure(
            CLUSTER,
            MEASURED_VALUE,
            ReportingConfig {
                min_interval: 10,
                max_interval: 60,
                reportable_change: 100,
            }
        ));

        // the first report has no configured time (2.5.11.2.1)
        assert!(table.should_report(CLUSTER, MEASURED_VALUE, 0, 2300));
        table.reported(CLUSTER, MEASURED_VALUE, 0, 2300);

        // inside the minimum interval nothing is reported, however big the
        // change
        assert!(!table.should_report(CLUSTER, MEASURED_VALUE, 5, 9999));
        // past the minimum, but neither the change nor the maximum reached
        assert!(!table.should_report(CLUSTER, MEASURED_VALUE, 20, 2350));
        // a big enough change
        assert!(table.should_report(CLUSTER, MEASURED_VALUE, 20, 2400));
        // or the maximum interval, with no change at all
        assert!(table.should_report(CLUSTER, MEASURED_VALUE, 60, 2300));
    }

    // 2.5.7.1.6: reporting is off while the maximum interval is 0xffff
    #[test]
    fn a_disabled_configuration_never_reports() {
        let table = table();
        table.configure(
            CLUSTER,
            MEASURED_VALUE,
            ReportingConfig {
                min_interval: 0,
                max_interval: MAX_INTERVAL_DISABLED,
                reportable_change: 0,
            },
        );
        assert!(!table.should_report(CLUSTER, MEASURED_VALUE, 1_000, 9999));
    }

    // an attribute nobody configured is left to the application's own policy
    #[test]
    fn an_unconfigured_attribute_is_never_due() {
        let table = table();
        assert!(!table.should_report(CLUSTER, MEASURED_VALUE, 1_000, 2300));
        assert_eq!(table.configuration(CLUSTER, MEASURED_VALUE), None);
    }

    #[test]
    fn reconfiguring_replaces_rather_than_fills_the_table() {
        let table = table();
        let config = ReportingConfig {
            min_interval: 1,
            max_interval: 2,
            reportable_change: 0,
        };
        assert!(table.configure(CLUSTER, MEASURED_VALUE, config));
        assert!(table.configure(CLUSTER, MEASURED_VALUE, config));
        assert_eq!(table.len(), 1);

        table.clear(CLUSTER, MEASURED_VALUE);
        assert!(table.is_empty());
    }

    #[test]
    fn a_full_table_rejects_further_configurations() {
        let table = table();
        let config = ReportingConfig {
            min_interval: 0,
            max_interval: 60,
            reportable_change: 0,
        };
        assert!(table.configure(CLUSTER, AttributeId(0), config));
        assert!(table.configure(CLUSTER, AttributeId(1), config));
        assert!(!table.configure(CLUSTER, AttributeId(2), config));
    }
}
