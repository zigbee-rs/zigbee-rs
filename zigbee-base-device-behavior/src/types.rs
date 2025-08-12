//! Common types and enums for Zigbee Base Device Behavior (BDB).
//!
//! See Section 5.2.1 and 5.2.9

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::LE;
use zigbee::nwk::nlme::management::NetworkDescriptor;

/// Commissioning modes
///
/// See Section 5.2.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommissioningMode {
    NetworkSteering = 0x01,
    NetworkFormation,
    FindingBinding,
    Touchlink,
}


/// Status codes for BDB commissioning
///
/// See Section 5.3.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BdbCommissioningStatus {
    /// Commissioning was successful.
    Success,
    /// One of the commissioning sub-procedures has started but is not yet complete.
    InProgress,
    /// The initiator is not address assignment capable during touchlink.
    NotAaCapable,
    /// No network was found during commissioning.
    NoNetwork,
    /// A node has not joined a network when requested during touchlink.
    TargetFailure,
    /// A network could not be formed during network formation.
    FormationFailure,
    /// No response to an identify query command has been received during finding & binding.
    NoIdentifyQueryResponse,
    /// A binding table entry could not be created due to insufficient space in the binding table during finding & binding.
    BindingTableFull,
    /// No response to a scan request inter-PAN command has been received during touchlink.
    NoScanResponse,
    /// A touchlink (steal) attempt was made when a node is already connected to a centralized security network.
    NotPermitted,
    /// The Trust Center link key exchange procedure has failed attempting to join a centralized security network.
    TclkExFailure,
    /// A commissioning procedure was forbidden since the node was not currently on a network.
    NotOnANetwork,
    /// A commissioning procedure was forbidden since the node was currently on a network.
    OnANetwork,
}


/// Events emitted during the network finding/joining process.
pub enum NetworkSteeringEvent {
    /// A network was found during scanning.
    ScanResult(NetworkDescriptor),
    /// Scanning for networks has started.
    ScanStarted,
    /// Scanning for networks has completed.
    ScanCompleted,
    /// Attempting to join a network.
    JoinInProgress(NetworkDescriptor),
    /// Successfully joined a network.
    JoinSuccess(NetworkDescriptor),
    /// Failed to join a network, with error details.
    JoinFailure {
        descriptor: Option<NetworkDescriptor>,
        error: JoinError,
    },
}

/// Possible errors during the join process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    /// No suitable networks found during scan.
    NoNetworksFound,
    /// Association request was rejected by the network.
    AssociationRejected,
    /// Timeout waiting for response from network.
    Timeout,
    /// Network is not accepting new devices (permit join = false).
    NotPermitted,
    /// Link quality too low to join.
    LinkQualityTooLow,
    /// Other or unknown error.
    Other(u8),
}


/// Trait for Zigbee Base Device Behavior: Network Steering (finding and joining
/// a network).
pub trait NetworkSteering {
    /// Start the network finding/joining process.
    ///
    /// The implementation should scan for available networks and attempt to
    /// join. Events are reported via the provided callback.
    fn start_network_steering<F>(&mut self, event_callback: F)
    where
        F: FnMut(NetworkSteeringEvent);
}
