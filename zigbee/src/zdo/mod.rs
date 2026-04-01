use config::Config;

pub mod config;
pub mod device_annce;
use crate::apl::descriptors::node_descriptor::LogicalType;
use crate::aps::apsme::Apsme;

/// Provides an interface between the application object, the device profile and
/// the APS.
pub struct ZigbeeDevice {
    config: Config,
    apsme: Apsme,
}

/// zigbee network
pub struct ZigBeeNetwork {}

impl ZigbeeDevice {
    /// Creates a new instance.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            apsme: Apsme::new(),
        }
    }

    /// Configures the device.
    pub fn configure(&self, _config: Config) {}

    /// Indicates if the device is connected to a zigbee network.
    pub fn is_connected(&self) -> bool {
        false // TODO: check connection state
    }

    pub fn logical_type(&self) -> LogicalType {
        self.config.device_type
    }

    pub fn send_keep_alive(&self) {}

    pub fn send_data(&self, _input: &[u8]) {}

    /// 2.1.3.1 - Device Discovery
    /// is the process whereby a ZigBee device can discover other ZigBee
    /// devices.
    pub fn start_device_discovery(&self) {
        match self.config.device_discovery_type {
            config::DiscoveryType::IEEE => {
                todo!()
                // TODO: send IEEE address request as unicast to a particular
                // device TODO: wait for incoming frames
            }
            config::DiscoveryType::NWK => {
                todo!()
                // TODO: send NWK address request as broadcast with the known
                // IEEE address as data payload TODO: wait for
                // incoming frames
            }
        }
    }

    /// 2.1.3.2 - Service Discovery
    /// is the process whereby the capabilities of a given device are discovered
    /// by other devices.
    pub fn start_service_discovery(&self) {}
}

impl Default for ZigbeeDevice {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
