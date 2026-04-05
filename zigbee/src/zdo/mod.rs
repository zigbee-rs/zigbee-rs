use config::Config;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

pub mod config;
pub mod device_annce;
use crate::apl::descriptors::node_descriptor::LogicalType;
use crate::aps::aib;
use crate::aps::aib::DeviceKeyPairDescriptor;
use crate::aps::aib::KeyAttribute;
use crate::aps::aib::LinkKeyType;
use crate::aps::apsme::Apsme;
use crate::aps::frame::CommandFrame;
use crate::aps::frame::Frame;
use crate::aps::frame::command::Command;
use crate::aps::frame::command::TransportKey;
use crate::nwk::nib;
use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::NlmeSap;
use crate::security::SecurityContext;
use zigbee_types::StorageVec;

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

    /// Broadcast a ZDO Device_annce (§2.4.3.1.11).
    pub async fn device_annce<N: NlmeSap>(
        &mut self,
        nlme: &mut N,
        annce: device_annce::DeviceAnnce,
    ) -> Result<(), NetworkError> {
        device_annce::broadcast(nlme, &mut self.apsme.aps_counter, annce).await
    }

    /// Security Manager: poll for a Transport-Key command and install the
    /// network key and Trust Center link key entry (§4.4.10).
    pub async fn poll_transport_key<N: NlmeSap>(
        &mut self,
        nlme: &mut N,
    ) -> Result<(), NetworkError> {
        let mut buf = [0u8; 128];
        let mut nwk_data = nlme.poll_nwk_data(&mut buf, 5).await?;

        // SAFETY: we can safely take a &mut since it references the buf above
        let aps_buf = unsafe { nwk_data.payload_as_mut() };
        let cx = SecurityContext::get();
        let aps_frame = cx.decrypt_aps_frame_in_place(aps_buf)?;

        let Frame::ApsCommand(CommandFrame {
            command: Command::TransportKey(transport_key),
            ..
        }) = aps_frame
        else {
            return Err(NetworkError::NoTransportKey);
        };

        match transport_key {
            TransportKey::StandardNetworkKey(nwk_key) => {
                log::debug!("[ZDO] received network key {:?}", nwk_key.key);

                let aib = aib::get_ref();
                aib.set_trust_center_address(nwk_key.source_address);
                let mut key_set = aib.device_key_pair_set();
                if !key_set
                    .iter()
                    .any(|k| k.device_address == nwk_key.source_address)
                {
                    let _ = key_set.push(DeviceKeyPairDescriptor {
                        device_address: nwk_key.source_address,
                        key_attributes: KeyAttribute::ProvisionalKey,
                        link_key: zigbee_types::ByteArray(crate::security::TRUST_CENTER_LINK_KEY),
                        outgoing_frame_counter: 0,
                        incoming_frame_counter: 0,
                        link_key_type: LinkKeyType::GlobalLinkKey,
                    });
                    aib.set_device_key_pair_set(key_set);
                }

                let nib = nib::get_ref();
                let mut sec_material = nib.security_material_set();
                sec_material.clear();
                let _ = sec_material.push(NetworkSecurityMaterialDescriptor {
                    key_seq_number: nwk_key.sequence_number,
                    outgoing_frame_counter: 0,
                    incoming_frame_counter_set: StorageVec::new(),
                    key: nwk_key.key,
                    network_key_type: 0x01,
                });
                nib.set_security_material_set(sec_material);
                nib.set_active_key_seq_number(nwk_key.sequence_number);
            }
            TransportKey::ApplicationLinkKey(_app_key) => (), // TODO
            TransportKey::TrustCenterLinkKey(_tcl_key) => (), // TODO
            TransportKey::Reserved(_) => return Err(NetworkError::NoTransportKey),
        }

        Ok(())
    }

    /// Security Manager: build and send an APS command frame (§4.4).
    ///
    /// Delegates to APSME which owns `apsCounter` (§4.4.11). When
    /// `aps_secure` is true the frame is APS-encrypted with the link key for
    /// `dest_ieee`; the NWK layer always applies network-key encryption.
    pub async fn send_aps_command<N: NlmeSap>(
        &mut self,
        nlme: &mut N,
        destination: ShortAddress,
        dest_ieee: IeeeAddress,
        command: Command,
        aps_secure: bool,
    ) -> Result<(), NetworkError> {
        self.apsme
            .send_command(nlme, destination, dest_ieee, command, aps_secure)
            .await
    }

    /// Security Manager: poll for an incoming APS command (§4.4).
    ///
    /// Delegates to APSME which decrypts the NWK and APS layers.
    pub async fn poll_aps_command<N: NlmeSap>(
        &mut self,
        nlme: &mut N,
        retries: u8,
    ) -> Result<Command, NetworkError> {
        self.apsme.poll_command(nlme, retries).await
    }
}

impl Default for ZigbeeDevice {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
