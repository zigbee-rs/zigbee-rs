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

use crate::impl_byte;

impl_byte! {
    #[tag(u8)]
    /// Comand Frame Identifiers.
    ///
    /// See Section 3.4.
    #[derive(Debug)]
    pub enum Command {
        RouteRequest = 0x01,
        RouteReply = 0x02,
        NetworkStatus = 0x03,
        Leave = 0x04,
        RouteRecord = 0x05,
        RejoinRequest = 0x06,
        RejoinResponse = 0x07,
        LinkStatus = 0x08,
        NetworkReport = 0x09,
        NetworkUpdate = 0x0a,
        EndDeviceTimeoutRequest = 0x0b,
        EndDeviceTimeoutResponse = 0x0c,
        LinkPowerDelta = 0x0d,
        #[fallback = true]
        Reserved(u8),
    }
}
