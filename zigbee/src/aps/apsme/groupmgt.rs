//! This set of primitives allows the next higher layer to manage group
//! membership for endpoints on the current device by adding and removing
//! entries in the group table
//!
//! 2.2.4.5 Group Management
type DstAddrMode = u8;
/// 2.2.4.3.1 - APSME-BIND.request
pub struct ApsmeAddrGroupRequest {
    group_address: u16,
    endpoint: u8,
}
