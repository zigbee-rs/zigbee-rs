//! NWK Command Frames

pub mod end_device_timeout_request;
pub mod end_device_timeout_response;
pub mod leave;
pub mod link_power_delta;
pub mod link_status;
pub mod network_report;
pub mod network_status;
pub mod network_update;
pub mod rejoin_request;
pub mod rejoin_response;
pub mod route_record;
pub mod route_reply;
pub mod route_request;

use zigbee_macros::impl_byte;

use crate::nwk::frame::command::end_device_timeout_request::EndDeviceTimeoutRequest;
use crate::nwk::frame::command::end_device_timeout_response::EndDeviceTimeoutResponse;
use crate::nwk::frame::command::leave::Leave;
use crate::nwk::frame::command::link_power_delta::LinkPowerDelta;
use crate::nwk::frame::command::link_status::LinkStatus;
use crate::nwk::frame::command::network_report::NetworkReport;
use crate::nwk::frame::command::network_status::NetworkStatus;
use crate::nwk::frame::command::network_update::NetworkUpdate;
use crate::nwk::frame::command::rejoin_request::RejoinRequest;
use crate::nwk::frame::command::rejoin_response::RejoinResponse;
use crate::nwk::frame::command::route_record::RouteRecord;
use crate::nwk::frame::command::route_reply::RouteReply;
use crate::nwk::frame::command::route_request::RouteRequest;

impl_byte! {
    #[tag(u8)]
    /// Command Frame Identifiers.
    ///
    /// See Section 3.4.
    #[derive(Debug, Clone)]
    pub enum Command<'a> {
        #[tag_value = 0x01]
        RouteRequest(RouteRequest),
        #[tag_value = 0x02]
        RouteReply(RouteReply),
        #[tag_value = 0x03]
        NetworkStatus(NetworkStatus),
        #[tag_value = 0x04]
        Leave(Leave),
        #[tag_value = 0x05]
        RouteRecord(RouteRecord<'a>),
        #[tag_value = 0x06]
        RejoinRequest(RejoinRequest),
        #[tag_value = 0x07]
        RejoinResponse(RejoinResponse),
        #[tag_value = 0x08]
        LinkStatus(LinkStatus<'a>),
        #[tag_value = 0x09]
        NetworkReport(NetworkReport<'a>),
        #[tag_value = 0x0a]
        NetworkUpdate(NetworkUpdate),
        #[tag_value = 0x0b]
        EndDeviceTimeoutRequest(EndDeviceTimeoutRequest),
        #[tag_value = 0x0c]
        EndDeviceTimeoutResponse(EndDeviceTimeoutResponse),
        #[tag_value = 0x0d]
        LinkPowerDelta(LinkPowerDelta<'a>),
        #[fallback = true]
        Reserved(u8),
    }
}
