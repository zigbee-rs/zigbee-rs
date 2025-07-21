//! Abstraction over the Zigbee stack for BDB operations.

use crate::types::NetworkDescriptor;

/// Abstraction over the Zigbee stack for BDB operations.
pub trait ZigbeeStack {
    /// Start a network scan (active or passive).
    fn start_network_scan(&mut self) -> Result<(), ZigbeeStackError>;

    /// Attempt to join a network with the given descriptor.
    fn join_network(&mut self, descriptor: &NetworkDescriptor) -> Result<(), ZigbeeStackError>;

    /// Form a new network (for coordinators/routers).
    fn form_network(&mut self, config: &NetworkFormationConfig) -> Result<(), ZigbeeStackError>;

    /// Leave the current network.
    fn leave_network(&mut self) -> Result<(), ZigbeeStackError>;

    /// Rejoin the network (with/without security).
    fn rejoin_network(&mut self, secure: bool) -> Result<(), ZigbeeStackError>;

    // ...add more as needed (e.g., permit join, set channel, etc.)
}

/// Error type for Zigbee stack operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZigbeeStackError {
    NotSupported,
    Timeout,
    Busy,
    InvalidState,
    HardwareError,
    Other(u8),
}

/// Example config for network formation (expand as needed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkFormationConfig {
    pub pan_id: Option<u16>,
    pub channel: Option<u8>,
    // ...add more fields as needed
}
