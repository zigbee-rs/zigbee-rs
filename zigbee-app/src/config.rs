//! Everything an application configures, in one place.
//!
//! The layers below take their own spec-shaped configuration — `zigbee::Config`
//! for the ZDO, `DeviceDescriptorConfig` for the descriptors an interviewer
//! asks for — and several values appear in more than one of them: the logical
//! device type, the MAC capability byte, the operating channel. [`StackConfig`]
//! is the single source for those, deriving the per-layer configuration from
//! it so the copies cannot drift apart.

use zigbee::nwk::nib::CapabilityInformation;
use zigbee::zdo::config::DiscoveryType;
use zigbee::zdo::descriptor::DeviceDescriptorConfig;
use zigbee_types::IeeeAddress;

/// The network this device belongs to.
pub struct NetworkConfig {
    /// Extended PAN id to join, rejoin, or steer onto.
    pub extended_pan_id: IeeeAddress,
    /// Channels scanned when joining, and when looking for a new parent
    /// (3.6.1.4.2). The first one is the operating channel.
    pub channels: core::ops::Range<u8>,
    /// Scan duration exponent used for those scans.
    pub scan_duration: u8,
}

/// What this device is on that network.
pub struct DeviceConfig {
    /// Coordinator, router, or end device.
    pub logical_type: zigbee::LogicalType,
    /// Capability information this device joins with (Table 3-62); its
    /// receiver-on-when-idle bit also decides how [`crate::rx_loop`] receives.
    pub capability_information: CapabilityInformation,
    /// How this device discovers addresses of others (2.4.3.1).
    pub discovery_type: DiscoveryType,
    /// Whether commissioning exchanges the initial Trust Center link key for a
    /// unique one (BDB 8.2 step 12). Disable for trust centers that never
    /// answer the request and would only stall the join.
    pub tc_link_key_exchange: bool,
}

/// Cadences the specification leaves to the implementer.
pub struct TimingConfig {
    /// How often a sleepy end device polls its parent for buffered frames.
    /// Keep it below the parent's indirect-transaction persistence time
    /// (~7.68 s).
    pub poll_interval_ms: u32,
    /// Keepalive period used until one is negotiated with the parent
    /// (3.6.10.3 leaves the period to the manufacturer); the negotiated
    /// timeout takes over as soon as there is one.
    pub default_keepalive_interval_ms: u32,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1_000,
            default_keepalive_interval_ms: 60_000,
        }
    }
}

/// The whole application-facing configuration of the stack.
///
/// Build it once with [`StackConfig::new`] and hand it to every entry point of
/// this crate; [`Self::zdo`] produces the configuration
/// [`zigbee::ZigbeeDevice`] and BDB expect.
pub struct StackConfig<'a> {
    network: NetworkConfig,
    device: DeviceConfig,
    timing: TimingConfig,
    descriptors: DeviceDescriptorConfig<'a>,
}

impl<'a> StackConfig<'a> {
    /// Assemble the configuration, deriving what the descriptors would
    /// otherwise duplicate: the node descriptor's logical type and MAC
    /// capability flags always match the ones the device joins with.
    pub fn new(
        network: NetworkConfig,
        device: DeviceConfig,
        timing: TimingConfig,
        descriptors: DeviceDescriptorConfig<'a>,
    ) -> Self {
        let mut descriptors = descriptors;
        descriptors.node.logical_type = device.logical_type;
        descriptors.node.mac_capability_flags = device.capability_information.0;
        Self {
            network,
            device,
            timing,
            descriptors,
        }
    }

    /// The network this device belongs to.
    pub fn network(&self) -> &NetworkConfig {
        &self.network
    }

    /// What this device is on that network.
    pub fn device(&self) -> &DeviceConfig {
        &self.device
    }

    /// The configured cadences.
    pub fn timing(&self) -> &TimingConfig {
        &self.timing
    }

    /// The descriptors served to an interviewer (2.3.2).
    pub fn descriptors(&self) -> &DeviceDescriptorConfig<'a> {
        &self.descriptors
    }

    /// The operating channel: the first of the configured channels.
    pub fn channel(&self) -> u8 {
        self.network.channels.start
    }

    /// The ZDO configuration derived from this one, for
    /// [`zigbee::ZigbeeDevice::new`] and `BaseDeviceBehavior::new`.
    pub fn zdo(&self) -> zigbee::Config {
        zigbee::Config {
            radio_channel: self.channel(),
            device_discovery_type: self.device.discovery_type,
            device_type: self.device.logical_type,
        }
    }
}
