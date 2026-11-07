//! Everything an application configures, in one place.
//!
//! The layers below take their own spec-shaped configuration —
//! `zigbee_core::Config` for the ZDO, `DeviceDescriptorConfig` for the
//! descriptors an interviewer asks for — and several values appear in more than
//! one of them: the logical device type, the MAC capability byte, the operating
//! channel. [`StackConfig`] is the single source for those, deriving the
//! per-layer configuration from it so the copies cannot drift apart.

use zigbee_core::nwk::nib::CapabilityInformation;
use zigbee_core::zdo::config::DiscoveryType;
use zigbee_core::zdo::descriptor::DeviceDescriptorConfig;
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
    pub logical_type: zigbee_core::LogicalType,
    /// Capability information this device joins with (Table 3-62); its
    /// receiver-on-when-idle bit also decides how [`crate::Stack::rx_loop`] receives.
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
    /// (~7.68 s). Capped at the negotiated keepalive interval when the parent
    /// tracks data polls as keepalives.
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

/// How often a failed rejoin is retried before the network is given up
/// (BDB 7.1 step 5 leaves this to the application).
///
/// Retrying bridges an outage: after a power cut a device is up before the
/// router it rejoins through. Applies at startup and to a parent link lost
/// later.
pub struct RejoinConfig {
    /// Rejoin attempts, the first one included. `None` never gives the
    /// network up.
    ///
    /// A finite count ends in steering, which is also what finds a network
    /// that moved to another channel (BDB 7.1 step 4 scans none).
    pub attempts: Option<u8>,
    /// Delay before the second attempt, doubling for each further one.
    pub backoff_ms: u32,
    /// Cap on that delay.
    pub max_backoff_ms: u32,
    /// Whether using up the attempts commissions the device again with
    /// network steering, which needs the network opened for joining.
    ///
    /// With `false` the device keeps what it needs to rejoin and reports
    /// [`crate::Commissioned::Failed`] instead. A device on no network steers
    /// either way.
    pub steer_after_failure: bool,
}

impl Default for RejoinConfig {
    /// Five attempts over roughly two minutes, then steering: long enough for
    /// a coordinator to reboot, short enough to re-commission by itself.
    fn default() -> Self {
        Self {
            attempts: Some(5),
            backoff_ms: 10_000,
            max_backoff_ms: 60_000,
            steer_after_failure: true,
        }
    }
}

/// The whole application-facing configuration of the stack.
///
/// Build it once with [`StackConfig::new`] and hand it to every entry point of
/// this crate; [`Self::zdo`] produces the configuration
/// [`zigbee_core::zdo::ZigbeeDevice`] and BDB expect.
pub struct StackConfig<'a> {
    network: NetworkConfig,
    device: DeviceConfig,
    timing: TimingConfig,
    rejoin: RejoinConfig,
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
            rejoin: RejoinConfig::default(),
            descriptors,
        }
    }

    /// Replace the rejoin retry schedule ([`RejoinConfig::default`] otherwise).
    #[must_use]
    pub fn with_rejoin(mut self, rejoin: RejoinConfig) -> Self {
        self.rejoin = rejoin;
        self
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

    /// How a failed rejoin is retried (BDB 7.1 step 5).
    pub fn rejoin(&self) -> &RejoinConfig {
        &self.rejoin
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
    /// [`zigbee_core::zdo::ZigbeeDevice::new`] and `BaseDeviceBehavior::new`.
    pub fn zdo(&self) -> zigbee_core::Config {
        zigbee_core::Config {
            radio_channel: self.channel(),
            device_discovery_type: self.device.discovery_type,
            device_type: self.device.logical_type,
        }
    }
}
