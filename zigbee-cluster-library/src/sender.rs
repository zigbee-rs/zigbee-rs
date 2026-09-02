//! ZCL sender helpers layered on top of the APSDE-SAP.
//!
//! Application code sending ZCL frames does not need to fill in APS addressing
//! details by hand: these helpers sit on top of
//! [`ZigbeeDevice::data_request`] (2.2.4.1.1) and turn a serialized ZCL frame
//! into a unicast APS data transfer in a single call.

use embedded_hal_async::delay::DelayNs;
use zigbee_core::aps::apsde::ApsdeSapConfirm;
use zigbee_core::aps::apsde::ApsdeSapRequest;
use zigbee_core::aps::types::Address;
use zigbee_core::aps::types::DstAddrMode;
use zigbee_core::aps::types::SrcEndpoint;
use zigbee_core::aps::types::TxOptions;
use zigbee_core::zdo::ZigbeeDevice;
use zigbee_mac::mlme::Mlme;

use crate::reporting::ReportFrame;

/// Errors produced when sending a ZCL frame.
#[derive(Debug)]
pub enum ZclSendError {
    /// Source endpoint outside the valid range (2.2.4.1.1).
    InvalidEndpoint,
}

/// APS-level addressing for a unicast ZCL transfer.
///
/// Mirrors the addressing fields of the APSDE-DATA.request primitive
/// (2.2.4.1.1) restricted to the [`ZclSender`] use case: a short address
/// destination, source/destination endpoints, and the profile/cluster pair.
#[derive(Debug, Clone, Copy)]
pub struct ZclUnicast {
    pub dst_short: u16,
    pub src_endpoint: u8,
    pub dst_endpoint: u8,
    pub profile_id: u16,
    pub cluster_id: u16,
}

/// APS-level addressing for an attribute report.
///
/// Mirrors [`ZclUnicast`] without the cluster identifier, which
/// [`ReportFrame`] already carries: the attributes in the frame determine the
/// cluster it is addressed to.
#[derive(Debug, Clone, Copy)]
pub struct ZclReportTarget {
    pub dst_short: u16,
    pub src_endpoint: u8,
    pub dst_endpoint: u8,
    pub profile_id: u16,
}

/// Convenience extension over [`ZigbeeDevice`] for sending ZCL frames.
pub trait ZclSender {
    /// Send a serialized ZCL frame as a unicast APS data frame (2.2.4.1.1).
    ///
    /// The transfer is acknowledged at the APS layer, so `delay` paces the
    /// wait for the acknowledgement and its retransmissions (2.2.8.4.4).
    async fn send_zcl_unicast(
        &self,
        addressing: ZclUnicast,
        asdu: &[u8],
        delay: &mut impl DelayNs,
    ) -> Result<ApsdeSapConfirm, ZclSendError>;

    /// Send an attribute report built by
    /// [`AttributeReportBuilder`](crate::reporting::AttributeReportBuilder).
    ///
    /// The cluster is taken from the report rather than from the caller, so a
    /// report cannot be addressed to a cluster its attributes do not belong
    /// to.
    async fn send_attribute_report(
        &self,
        target: ZclReportTarget,
        report: ReportFrame<'_>,
        delay: &mut impl DelayNs,
    ) -> Result<ApsdeSapConfirm, ZclSendError> {
        self.send_zcl_unicast(
            ZclUnicast {
                dst_short: target.dst_short,
                src_endpoint: target.src_endpoint,
                dst_endpoint: target.dst_endpoint,
                profile_id: target.profile_id,
                cluster_id: report.cluster_id().0,
            },
            report.asdu(),
            delay,
        )
        .await
    }
}

impl<M: Mlme> ZclSender for ZigbeeDevice<M> {
    async fn send_zcl_unicast(
        &self,
        addressing: ZclUnicast,
        asdu: &[u8],
        delay: &mut impl DelayNs,
    ) -> Result<ApsdeSapConfirm, ZclSendError> {
        let src =
            SrcEndpoint::new(addressing.src_endpoint).map_err(|_| ZclSendError::InvalidEndpoint)?;

        let request = ApsdeSapRequest {
            dst_addr_mode: DstAddrMode::Network,
            dst_address: Address::Network(addressing.dst_short),
            dst_endpoint: addressing.dst_endpoint,
            profile_id: addressing.profile_id,
            cluster_id: addressing.cluster_id,
            src_endpoint: src,
            asdu,
            tx_options: TxOptions::default(),
            ..Default::default()
        };

        Ok(self.data_request(request, delay).await)
    }
}
