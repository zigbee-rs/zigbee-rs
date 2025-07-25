//! Common types and enums for Zigbee Base Device Behavior (BDB).
//!
//! See Section 5.2.1 and 5.2.9

use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use byte::LE;

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


/// Represents the result of a network scan (beacon) as per Zigbee BDB spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkDescriptor {
    /// 16-bit PAN ID of the network.
    pub pan_id: u16,
    /// 64-bit Extended PAN ID (unique identifier for the Zigbee network).
    pub extended_pan_id: [u8; 8],
    /// Logical channel number (11-26 for 2.4GHz Zigbee).
    pub channel: u8,
    /// Link Quality Indicator (LQI) for the received beacon (0-255).
    pub lqi: u8,
    /// Stack profile (e.g., Zigbee Pro = 2).
    pub stack_profile: u8,
    /// Zigbee protocol version (e.g., 0x02 for Zigbee 2007, 0x22 for Zigbee
    /// Pro).
    pub zigbee_version: u8,
    /// Whether the network is currently allowing new devices to join.
    pub permit_joining: bool,
    /// Device depth in the network tree (0 = coordinator, 1 = router, etc.).
    pub depth: u8,
    /// Whether the device has capacity to accept new routers.
    pub router_capacity: bool,
    /// Whether the device has capacity to accept new end devices.
    pub end_device_capacity: bool,
}

impl<'a> TryRead<'a, ()> for NetworkDescriptor {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let pan_id = bytes.read_with(offset, LE)?;
        let mut extended_pan_id = [0u8; 8];
        extended_pan_id.copy_from_slice(bytes.read_with(offset, byte::ctx::Bytes::Len(8))?);
        let channel = bytes.read_with(offset, LE)?;
        let lqi = bytes.read_with(offset, LE)?;
        let stack_profile = bytes.read_with(offset, LE)?;
        let zigbee_version = bytes.read_with(offset, LE)?;
        let permit_joining: u8 = bytes.read_with(offset, LE)?;
        let permit_joining = permit_joining != 0;
        let depth = bytes.read_with(offset, LE)?;
        let router_capacity: u8 = bytes.read_with(offset, LE)?;
        let router_capacity = router_capacity != 0;
        let end_device_capacity: u8 = bytes.read_with(offset, LE)?;
        let end_device_capacity = end_device_capacity != 0;

        Ok((
            Self {
                pan_id,
                extended_pan_id,
                channel,
                lqi,
                stack_profile,
                zigbee_version,
                permit_joining,
                depth,
                router_capacity,
                end_device_capacity,
            },
            *offset,
        ))
    }
}

impl TryWrite for NetworkDescriptor {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write_with(offset, self.pan_id, LE)?;
        bytes.write_with(offset, &self.extended_pan_id[..], ())?;
        bytes.write_with(offset, self.channel, LE)?;
        bytes.write_with(offset, self.lqi, LE)?;
        bytes.write_with(offset, self.stack_profile, LE)?;
        bytes.write_with(offset, self.zigbee_version, LE)?;
        bytes.write_with(offset, self.permit_joining as u8, LE)?;
        bytes.write_with(offset, self.depth, LE)?;
        bytes.write_with(offset, self.router_capacity as u8, LE)?;
        bytes.write_with(offset, self.end_device_capacity as u8, LE)?;
        Ok(*offset)
    }
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

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use super::*;

    fn sample_descriptor() -> NetworkDescriptor {
        NetworkDescriptor {
            pan_id: 0x1234,
            extended_pan_id: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
            channel: 15,
            lqi: 200,
            stack_profile: 2,
            zigbee_version: 0x22,
            permit_joining: true,
            depth: 1,
            router_capacity: true,
            end_device_capacity: false,
        }
    }

    #[test]
    fn test_try_write_and_read_roundtrip() {
        let desc = sample_descriptor();
        let mut buf = [0u8; 16];
        let written = desc.clone().try_write(&mut buf, ()).unwrap();
        let (parsed, read) = NetworkDescriptor::try_read(&buf[..written], ()).unwrap();
        assert_eq!(written, read);
        assert_eq!(desc, parsed);
    }

    #[test]
    fn test_all_fields_false() {
        let desc = NetworkDescriptor {
            pan_id: 0,
            extended_pan_id: [0; 8],
            channel: 0,
            lqi: 0,
            stack_profile: 0,
            zigbee_version: 0,
            permit_joining: false,
            depth: 0,
            router_capacity: false,
            end_device_capacity: false,
        };
        let mut buf = [0u8; 16];
        let written = desc.clone().try_write(&mut buf, ()).unwrap();
        let (parsed, read) = NetworkDescriptor::try_read(&buf[..written], ()).unwrap();
        assert_eq!(written, read);
        assert_eq!(desc, parsed);
    }

    #[test]
    fn test_all_fields_true_max() {
        let desc = NetworkDescriptor {
            pan_id: u16::MAX,
            extended_pan_id: [0xFF; 8],
            channel: u8::MAX,
            lqi: u8::MAX,
            stack_profile: u8::MAX,
            zigbee_version: u8::MAX,
            permit_joining: true,
            depth: u8::MAX,
            router_capacity: true,
            end_device_capacity: true,
        };
        let mut buf = [0u8; 16];
        let written = desc.clone().try_write(&mut buf, ()).unwrap();
        let (parsed, read) = NetworkDescriptor::try_read(&buf[..written], ()).unwrap();
        assert_eq!(written, read);
        assert_eq!(desc, parsed);
    }

    #[test]
    fn test_invalid_buffer_too_short() {
        let buf = [0u8; 5];
        let result = NetworkDescriptor::try_read(&buf, ());
        assert!(result.is_err());
    }
}
