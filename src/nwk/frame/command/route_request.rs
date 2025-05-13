use crate::common::types::IeeeAddress;
use crate::common::types::ShortAddress;

pub struct RouteRequest {
    command_options: u8,
    route_request_id: u8,
    destination_address: ShortAddress,
    path_cost: u8,
    destination_ieee_address: Option<IeeeAddress>,
}
