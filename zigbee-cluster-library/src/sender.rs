//! ZCL sender helpers layered on top of the APSDE-SAP.
//!
//! Application code building ZCL frames does not need to manage serialization
//! buffers or APS addressing details by hand: implement these helpers on top
//! of [`ZigbeeDevice::data_request`] (§2.2.4.1.1) and turn a [`ZclFrame`]
//! into a unicast APS data transfer in a single call.

use byte::BytesExt;
use heapless::Vec;
use zigbee::aps::apsde::ApsdeSapConfirm;
use zigbee::aps::apsde::ApsdeSapRequest;
use zigbee::aps::types::Address;
use zigbee::aps::types::DstAddrMode;
use zigbee::aps::types::SrcEndpoint;
use zigbee::aps::types::TxOptions;
use zigbee::zdo::ZigbeeDevice;
use zigbee_mac::mlme::Mlme;

use crate::common::data_types::ZclDataType;
use crate::frame::AttributeReport;
use crate::frame::GeneralCommand;
use crate::frame::ZclFrame;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameControl;
use crate::payload::ZclFramePayload;

/// Maximum ZCL frame size used for the internal serialization buffer.
///
/// Matches the working buffer in [`zigbee::aps::apsme::Apsme::unicast_data`]
/// minus a conservative APS header allowance.
const ZCL_TX_BUF_SIZE: usize = 96;

/// Errors produced when building or sending a ZCL frame.
#[derive(Debug)]
pub enum ZclSendError {
    /// Source endpoint outside the valid range (§2.2.4.1.1).
    InvalidEndpoint,
    /// ZCL frame did not fit into the internal serialization buffer.
    EncodeError(byte::Error),
}

impl From<byte::Error> for ZclSendError {
    fn from(value: byte::Error) -> Self {
        Self::EncodeError(value)
    }
}

/// APS-level addressing for a unicast ZCL transfer.
///
/// Mirrors the addressing fields of the APSDE-DATA.request primitive
/// (§2.2.4.1.1) restricted to the [`ZclSender`] use case: a short address
/// destination, source/destination endpoints, and the profile/cluster pair.
#[derive(Debug, Clone, Copy)]
pub struct ZclUnicast {
    pub dst_short: u16,
    pub src_endpoint: u8,
    pub dst_endpoint: u8,
    pub profile_id: u16,
    pub cluster_id: u16,
}

/// Convenience extension over [`ZigbeeDevice`] for sending ZCL frames.
pub trait ZclSender {
    /// Serialize a [`ZclFrame`] and send it as a unicast APS data frame
    /// (§2.2.4.1.1) to the given short address.
    async fn send_zcl_unicast(
        &self,
        addressing: ZclUnicast,
        frame: ZclFrame<'_>,
    ) -> Result<ApsdeSapConfirm, ZclSendError>;
}

impl<M: Mlme> ZclSender for ZigbeeDevice<M> {
    async fn send_zcl_unicast(
        &self,
        addressing: ZclUnicast,
        frame: ZclFrame<'_>,
    ) -> Result<ApsdeSapConfirm, ZclSendError> {
        let src =
            SrcEndpoint::new(addressing.src_endpoint).map_err(|_| ZclSendError::InvalidEndpoint)?;

        let mut buf = [0u8; ZCL_TX_BUF_SIZE];
        let offset = &mut 0;
        buf.write_with(offset, frame, ())?;

        let request = ApsdeSapRequest {
            dst_addr_mode: DstAddrMode::Network,
            dst_address: Address::Network(addressing.dst_short),
            dst_endpoint: addressing.dst_endpoint,
            profile_id: addressing.profile_id,
            cluster_id: addressing.cluster_id,
            src_endpoint: src,
            asdu: &buf[..*offset],
            tx_options: TxOptions::default(),
            ..Default::default()
        };

        Ok(self.data_request(request).await)
    }
}

/// Builds a [`ZclFrame`] carrying a `Report Attributes` general command
/// (ZCL §2.5.11), with the server→client direction bit set and default
/// response disabled — the typical configuration for a sensor reporting
/// to the trust center.
///
/// Returns an encoding error if the supplied reports exceed
/// [`GeneralCommand::ReportAttributes`]'s record capacity.
pub fn build_report_attributes<'a, I>(
    sequence_number: u8,
    reports: I,
) -> Result<ZclFrame<'a>, byte::Error>
where
    I: IntoIterator<Item = (u16, ZclDataType<'a>)>,
{
    let mut records: Vec<AttributeReport<'a>, 16> = Vec::new();
    for (attribute_id, value) in reports {
        records
            .push(AttributeReport {
                attribute_id,
                value,
            })
            .map_err(|_| bad_input!("report attributes capacity exceeded"))?;
    }

    Ok(ZclFrame {
        header: ZclHeader {
            // Global cmd, server→client, default response disabled.
            frame_control: FrameControl(0x18),
            manufacturer_code: None,
            sequence_number,
            command_identifier: CommandIdentifier::ReportAttributes,
        },
        payload: ZclFramePayload::GeneralCommand(GeneralCommand::ReportAttributes(records)),
    })
}
