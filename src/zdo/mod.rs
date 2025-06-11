use config::Config;

pub mod config;
use crate::aps::apsde::request::ApsdeSapRequest;
use crate::aps::apsde::sap::ApsdeSap;
use crate::aps::apsde::Apsde;
use crate::aps::apsme::Apsme;
use crate::aps::types::TxOptions;

/// provides an interface between the appication object, the device profile and
/// the APS
pub struct ZigbeeDevice {
    config: Config,
    apsme: Apsme,
    apsde: Apsde,
}

/// zigbee network
pub struct ZigBeeNetwork {}

impl ZigbeeDevice {
    /// creates a new instance
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            apsme: Apsme::new(),
            apsde: Apsde::new(),
        }
    }

    /// configures the device
    pub fn configure(&self, _config: Config) {}

    /// Indicates if the device is connected to a zigbee network
    pub fn is_connected(&self) -> bool {
        false // TODO: check connection state
    }

    /// scans for nearby reachable networks by sending a beacon request
    pub fn scan_for_available_networks(&self) {
        self.apsme.start_network_discovery();
        // TODO: send beacon requests to actively scan for networks
        // TODO: Beacon response (signal strenght - RSSI, network PAN ID, permit
        // to join)
    }

    /// tries to connect to the first reachable network by sending a join
    /// request
    pub fn try_to_connect(&self) {
        self.apsme.join_network();
        // TODO: send Association request to choosen network coordinator or
        // router TODO coordinator/router responds with an association
        // confirmation
    }

    pub fn setup_security(&self) {
        // TODO: exchange security keys (pre-configured trust center link keys)
    }

    pub fn send_keep_alive(&self) {}

    pub fn send_data(&self, _data: &[u8]) {
        self.apsde.data_request(ApsdeSapRequest {
            dst_addr_mode: crate::aps::types::DstAddrMode::default(),
            dst_address: crate::aps::types::Address::default(),
            dst_endpoint: 0,
            profile_id: 0x0000,
            cluster_id: 0x0000,
            src_endpoint: crate::aps::types::SrcEndpoint::default(),
            asdulength: 0,
            asdu: 0,
            tx_options: TxOptions::default(),
            use_alias: false,
            alias_src_addr: 0,
            alias_seq_number: 0,
            radius_counter: 0,
        });
    }

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
        Self::new()
    }
}
