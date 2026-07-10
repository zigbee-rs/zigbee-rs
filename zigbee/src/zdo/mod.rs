use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use config::Config;
use embedded_hal_async::delay::DelayNs;
use zigbee_mac::mlme::Mlme;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

pub mod config;
pub mod descriptor;
pub mod device_annce;
use zigbee_types::StorageVec;
use zigbee_types::sync::Event;

use crate::apl::descriptors::node_descriptor::LogicalType;
use crate::aps::aib;
use crate::aps::aib::DeviceKeyPairDescriptor;
use crate::aps::aib::KeyAttribute;
use crate::aps::aib::LinkKeyType;
use crate::aps::apsde::ApsdeSapConfirm;
use crate::aps::apsde::ApsdeSapConfirmStatus;
use crate::aps::apsde::ApsdeSapIndication;
use crate::aps::apsde::ApsdeSapRequest;
use crate::aps::apsme::Apsme;
use crate::aps::frame::CommandFrame;
use crate::aps::frame::Frame;
use crate::aps::frame::command::Command;
use crate::aps::frame::command::TransportKey;
use crate::nwk::nib;
use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::Nlme;
use crate::security::SecurityContext;

/// Number of MAC poll attempts when waiting for the Trust Center to deliver the
/// network key (§4.4.10). When the parent is a router (not the coordinator) the
/// key must travel router -> Trust Center -> router before the parent can
/// buffer it for a sleepy child, so the window must be wide enough to cover
/// that round-trip. Each empty poll already costs ~aResponseWaitTime, which
/// paces the retries.
const TRANSPORT_KEY_POLL_RETRIES: u8 = 20;

/// Provides an interface between the application object, the device profile and
/// the APS.
///
/// Owns the NWK and APS layers behind shared (`&self`) access: all methods take
/// `&self` and rely on the MAC's interior mutability plus atomic counters, so a
/// single `ZigbeeDevice` can be shared (e.g. as `&'static`) between a receive
/// loop and the application's transmit path.
pub struct ZigbeeDevice<M> {
    config: Config,
    nlme: Nlme<M>,
    apsme: Apsme,
    /// ZDP transaction sequence number (§2.4.2), independent of the APS
    /// counter.
    zdp_seq: AtomicU8,
    /// signaled once the network key is installed; gates the receive loop
    joined: Event,
}

/// zigbee network
pub struct ZigBeeNetwork {}

/// ZigBee Device Profile identifier (endpoint 0).
const ZDP_PROFILE_ID: u16 = 0x0000;
/// ZDO endpoint.
const ZDO_ENDPOINT: u8 = 0x00;

/// Handler for inbound application-profile (non-ZDP) requests.
///
/// Implemented by cluster servers (e.g. the ZCL Basic cluster) to answer
/// requests delivered to the [`ZigbeeDevice::rx_loop`]. Takes `&self` so it can
/// be shared with the receive task.
pub trait ClusterRequestHandler {
    /// Build a response ASDU for an application-profile request, writing it
    /// into `out` and returning its length, or `None` for no reply.
    fn handle(
        &self,
        profile_id: u16,
        cluster_id: u16,
        src_endpoint: u8,
        dst_endpoint: u8,
        asdu: &[u8],
        out: &mut [u8],
    ) -> Option<usize>;
}

/// Chains two handlers: the first to produce a response wins. Lets an
/// application compose independent cluster servers (e.g. Basic-cluster reads
/// plus a generic reporting responder) into the single handler
/// [`ZigbeeDevice::rx_loop`] expects.
impl<A: ClusterRequestHandler, B: ClusterRequestHandler> ClusterRequestHandler for (A, B) {
    fn handle(
        &self,
        profile_id: u16,
        cluster_id: u16,
        src_endpoint: u8,
        dst_endpoint: u8,
        asdu: &[u8],
        out: &mut [u8],
    ) -> Option<usize> {
        if let Some(len) = self.0.handle(
            profile_id,
            cluster_id,
            src_endpoint,
            dst_endpoint,
            asdu,
            out,
        ) {
            return Some(len);
        }
        self.1.handle(
            profile_id,
            cluster_id,
            src_endpoint,
            dst_endpoint,
            asdu,
            out,
        )
    }
}

impl<M: Mlme> ZigbeeDevice<M> {
    /// Creates a new instance owning the given NWK management entity.
    pub fn new(config: Config, nlme: Nlme<M>) -> Self {
        Self {
            config,
            nlme,
            apsme: Apsme::new(),
            zdp_seq: AtomicU8::new(0),
            joined: Event::new(),
        }
    }

    /// Forgets the joined network so the next boot re-commissions: clears the
    /// network address and security material.
    ///
    /// With flash-backed information bases, flush the storage before
    /// resetting so this state survives the reboot.
    pub fn forget_network(&self) {
        let nib = nib::get_ref();
        nib.update_network_address(|value| *value = 0xffff);
        nib.update_security_material_set(|set| set.clear());
    }

    /// Access the owned NWK management entity (used by BDB during
    /// commissioning).
    pub fn nlme(&self) -> &Nlme<M> {
        &self.nlme
    }

    /// Next ZDP transaction sequence number (§2.4.2), wrapping.
    fn next_zdp_seq(&self) -> u8 {
        self.zdp_seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
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

    /// APSDE-DATA.request (§2.2.4.1.1).
    ///
    /// Builds an APS data frame for the given destination + endpoint +
    /// cluster + profile and hands it to the NWK layer for encryption with
    /// the active network key and transmission via the parent.
    ///
    /// MVP scope: only [`DstAddrMode::Network`] (16-bit short address) is
    /// supported. Indirect (binding-resolved) addressing, group, and
    /// extended addressing return [`ApsdeSapConfirmStatus::Unsupported`].
    pub async fn data_request(&self, request: ApsdeSapRequest<'_>) -> ApsdeSapConfirm {
        use crate::aps::types::Address;
        use crate::aps::types::DstAddrMode;

        let status = match (request.dst_addr_mode, request.dst_address) {
            (DstAddrMode::Network, Address::Network(short)) => {
                match self
                    .apsme
                    .unicast_data(
                        &self.nlme,
                        ShortAddress(short),
                        request.dst_endpoint,
                        request.cluster_id,
                        request.profile_id,
                        request.src_endpoint.value,
                        request.asdu,
                    )
                    .await
                {
                    Ok(()) => ApsdeSapConfirmStatus::Success,
                    Err(_) => ApsdeSapConfirmStatus::NoAck,
                }
            }
            _ => ApsdeSapConfirmStatus::Unsupported,
        };

        ApsdeSapConfirm {
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
            src_endpoint: request.src_endpoint,
            status,
            tx_time: 0,
        }
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

    /// Broadcast a ZDO Device_annce (§2.4.3.1.11).
    pub async fn device_annce(&self, annce: device_annce::DeviceAnnce) -> Result<(), NetworkError> {
        device_annce::broadcast(&self.nlme, &self.apsme, self.next_zdp_seq(), annce).await
    }

    /// Security Manager: poll for a Transport-Key command and install the
    /// network key and Trust Center link key entry (§4.4.10).
    ///
    /// Pre-NWK-key join path only: the frame is decrypted with the well-known
    /// key before the network key is installed. Signals
    /// [`Self::wait_until_joined`] on success, releasing a gated receive
    /// loop, which handles all APS commands from then on.
    pub async fn poll_transport_key(&self) -> Result<(), NetworkError> {
        let mut buf = [0u8; 128];
        let mut nwk_data = self
            .nlme
            .poll_nwk_data(&mut buf, TRANSPORT_KEY_POLL_RETRIES)
            .await?;

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
                log::debug!("[ZDO] received network key {:02x?}", nwk_key.key);

                let aib = aib::get_ref();
                aib.update_trust_center_address(|value| *value = nwk_key.source_address);
                aib.update_device_key_pair_set(|key_set| {
                    if !key_set
                        .iter()
                        .any(|k| k.device_address == nwk_key.source_address)
                    {
                        let _ = key_set.push(DeviceKeyPairDescriptor {
                            device_address: nwk_key.source_address,
                            key_attributes: KeyAttribute::ProvisionalKey,
                            link_key: zigbee_types::ByteArray(
                                crate::security::TRUST_CENTER_LINK_KEY,
                            ),
                            outgoing_frame_counter: 0,
                            incoming_frame_counter: 0,
                            link_key_type: LinkKeyType::GlobalLinkKey,
                        });
                    }
                });

                let nib = nib::get_ref();
                nib.update_security_material_set(|sec_material| {
                    sec_material.clear();
                    let _ = sec_material.push(NetworkSecurityMaterialDescriptor {
                        key_seq_number: nwk_key.sequence_number,
                        outgoing_frame_counter: 0,
                        incoming_frame_counter_set: StorageVec::new(),
                        key: nwk_key.key,
                        network_key_type: 0x01,
                    });
                });
                nib.update_active_key_seq_number(|value| *value = nwk_key.sequence_number);
                self.joined.signal();
            }
            // only the network key completes the join (§4.4.10); anything
            // else here is a stale frame — fail instead of proceeding keyless
            TransportKey::ApplicationLinkKey(_)
            | TransportKey::TrustCenterLinkKey(_)
            | TransportKey::Reserved(_) => return Err(NetworkError::NoTransportKey),
        }

        Ok(())
    }

    /// Security Manager: build and send an APS command frame (§4.4).
    ///
    /// Delegates to APSME which owns `apsCounter` (§4.4.11). When
    /// `aps_secure` is true the frame is APS-encrypted with the link key for
    /// `dest_ieee`; the NWK layer always applies network-key encryption.
    pub async fn send_aps_command(
        &self,
        destination: ShortAddress,
        dest_ieee: IeeeAddress,
        command: Command,
        aps_secure: bool,
    ) -> Result<(), NetworkError> {
        self.apsme
            .send_command(&self.nlme, destination, dest_ieee, command, aps_secure)
            .await
    }

    /// Wait until the network key is installed and the device is joined.
    ///
    /// Gates the receive loop: spawn the RX task before commissioning and
    /// await this first, so the parent is not polled while the join is still
    /// in progress.
    pub async fn wait_until_joined(&self) {
        self.joined.wait_set().await;
    }

    /// Arm the TC link key exchange (BDB §10.2.5): key-transport commands
    /// are honored and stale progress events are cleared. Disarm with
    /// [`Self::end_tc_link_key_exchange`] when the exchange concludes.
    pub fn begin_tc_link_key_exchange(&self) {
        self.apsme.begin_tc_key_exchange();
    }

    /// Disarm the TC link key exchange; unsolicited key-transport commands
    /// are ignored again.
    pub fn end_tc_link_key_exchange(&self) {
        self.apsme.end_tc_key_exchange();
    }

    /// Wait until the Trust Center delivered a new link key (§4.4.10).
    ///
    /// The receive loop installs the key as unverified into the AIB before
    /// signaling. Bound the wait with an executor timeout.
    pub async fn wait_tc_link_key_received(&self) {
        self.apsme.tc_key_received.wait().await;
    }

    /// Wait until the Trust Center answered the key verification (§4.4.9)
    /// and return the Confirm-Key status (0x00 = success, key marked
    /// verified).
    ///
    /// Bound the wait with an executor timeout.
    pub async fn wait_tc_link_key_verified(&self) -> u8 {
        self.apsme.tc_key_verified.wait().await;
        self.apsme.tc_confirm_status()
    }

    /// Receive and dispatch one inbound APS data frame.
    ///
    /// ZDP requests (profile 0x0000) are answered internally from `cfg`;
    /// application-profile frames are delegated to `handler`. Any produced
    /// response is unicast back to the requester.
    pub async fn poll_and_dispatch(
        &self,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        handler: &impl ClusterRequestHandler,
    ) -> Result<(), NetworkError> {
        let mut rx = [0u8; 256];
        let indication = match self.apsme.poll_aps_frame(&self.nlme, &mut rx).await {
            Ok(indication) => indication,
            // ambient traffic we cannot decode (foreign or pre-key frames caught
            // in the receive window) — normal, not a fault.
            Err(NetworkError::ParseError | NetworkError::SecurityError(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        self.dispatch(indication, cfg, handler).await
    }

    /// Passively receive and dispatch one inbound APS data frame (rx-on-when-
    /// idle devices), like [`Self::poll_and_dispatch`] without polling the
    /// parent.
    pub async fn receive_and_dispatch(
        &self,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        handler: &impl ClusterRequestHandler,
    ) -> Result<(), NetworkError> {
        let mut rx = [0u8; 256];
        let indication = match self.apsme.receive_aps_frame(&self.nlme, &mut rx).await {
            Ok(indication) => indication,
            // ambient traffic we cannot decode — normal, not a fault
            Err(NetworkError::ParseError | NetworkError::SecurityError(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        self.dispatch(indication, cfg, handler).await
    }

    /// Answer one dispatched APSDE-DATA.indication, if any.
    async fn dispatch(
        &self,
        indication: Option<ApsdeSapIndication<'_>>,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        handler: &impl ClusterRequestHandler,
    ) -> Result<(), NetworkError> {
        use crate::aps::types::Address;

        let mut out = [0u8; 128];

        // non-dispatchable frame (NWK command handled by the NWK layer; an APS
        // command is handled at its arrival point): wait for the next frame
        let Some(indication) = indication else {
            return Ok(());
        };
        let Address::Network(src) = indication.src_address else {
            return Ok(());
        };
        let profile = indication.profile_id;
        let cluster = indication.cluster_id;
        let src_endpoint = indication.src_endpoint.value;
        let dst_endpoint = indication.dst_endpoint;

        log::debug!(
            "[ZDO] rx profile={profile:#06x} cluster={cluster:#06x} from {src:#06x} ep {src_endpoint}->{dst_endpoint}"
        );

        // (response cluster, profile, dst endpoint, src endpoint, length)
        let response = if profile == ZDP_PROFILE_ID {
            self.build_zdp_response(cluster, indication.asdu, cfg, &mut out)
                .map(|(rsp_cluster, len)| {
                    (rsp_cluster, ZDP_PROFILE_ID, ZDO_ENDPOINT, ZDO_ENDPOINT, len)
                })
        } else {
            handler
                .handle(
                    profile,
                    cluster,
                    src_endpoint,
                    dst_endpoint,
                    indication.asdu,
                    &mut out,
                )
                // application reply: same cluster/profile, endpoints swapped
                .map(|len| (cluster, profile, src_endpoint, dst_endpoint, len))
        };

        if let Some((rsp_cluster, rsp_profile, rsp_dst_ep, rsp_src_ep, len)) = response {
            log::debug!("[ZDO] tx response cluster={rsp_cluster:#06x} ({len} bytes) to {src:#06x}");
            self.apsme
                .unicast_data(
                    &self.nlme,
                    ShortAddress(src),
                    rsp_dst_ep,
                    rsp_cluster,
                    rsp_profile,
                    rsp_src_ep,
                    &out[..len],
                )
                .await?;
        } else {
            log::trace!("[ZDO] no response for cluster {cluster:#06x}");
        }
        Ok(())
    }

    /// Build the ZDP `*_rsp` payload for a discovery request, echoing the
    /// request's transaction sequence number. Returns the response cluster id
    /// and payload length, or `None` for unsupported requests.
    fn build_zdp_response(
        &self,
        cluster: u16,
        asdu: &[u8],
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        out: &mut [u8],
    ) -> Option<(u16, usize)> {
        let seq = *asdu.first()?;
        let nwk_addr = *self.nlme.nib().network_address();
        let ieee_addr = *self.nlme.nib().ieee_address();

        let result = match cluster {
            descriptor::NODE_DESC_REQ => (
                descriptor::NODE_DESC_RSP,
                cfg.node_desc_rsp(seq, nwk_addr, out).ok()?,
            ),
            descriptor::ACTIVE_EP_REQ => (
                descriptor::ACTIVE_EP_RSP,
                cfg.active_ep_rsp(seq, nwk_addr, out).ok()?,
            ),
            descriptor::SIMPLE_DESC_REQ => {
                // asdu: seq(1) + NWKAddrOfInterest(2) + endpoint(1)
                let endpoint = *asdu.get(3)?;
                (
                    descriptor::SIMPLE_DESC_RSP,
                    cfg.simple_desc_rsp(seq, nwk_addr, endpoint, out).ok()?,
                )
            }
            descriptor::IEEE_ADDR_REQ => (
                descriptor::IEEE_ADDR_RSP,
                descriptor::addr_rsp(seq, ieee_addr, nwk_addr, out).ok()?,
            ),
            descriptor::NWK_ADDR_REQ => (
                descriptor::NWK_ADDR_RSP,
                descriptor::addr_rsp(seq, ieee_addr, nwk_addr, out).ok()?,
            ),
            // asdu: seq(1) + SrcAddress(8) + SrcEndp(1) + ... — only the source
            // endpoint is needed to ack; the binding itself is not persisted
            // (this device reports unconditionally rather than from the table).
            descriptor::BIND_REQ => (
                descriptor::BIND_RSP,
                descriptor::bind_rsp(seq, *asdu.get(9)?, out).ok()?,
            ),
            descriptor::UNBIND_REQ => (
                descriptor::UNBIND_RSP,
                descriptor::bind_rsp(seq, *asdu.get(9)?, out).ok()?,
            ),
            _ => return None,
        };
        Some(result)
    }

    /// Run the receive/dispatch loop forever (the steady-state RX task).
    ///
    /// Waits for the join to complete before touching the radio, so it can
    /// be spawned ahead of commissioning. The receive strategy follows the
    /// joined capability (`nwkCapabilityInformation`, §3.4.1.3.1.1): a device
    /// with `rxOnWhenIdle = TRUE` listens for frames directly, while a sleepy
    /// end device polls the parent for buffered unicasts every
    /// `poll_interval_ms` — keep it below the parent's indirect-transaction
    /// persistence time (~7.68 s).
    pub async fn rx_loop(
        &self,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        handler: &impl ClusterRequestHandler,
        delay: &mut impl DelayNs,
        poll_interval_ms: u32,
    ) -> ! {
        // a restored network address means the device resumed on a network
        // without a fresh key exchange — release the gate immediately
        if *self.nlme.nib().network_address() != ShortAddress::default().0 {
            self.joined.signal();
        }
        self.wait_until_joined().await;
        if self
            .nlme
            .nib()
            .capability_information()
            .receiver_on_when_idle()
        {
            loop {
                if let Err(e) = self.receive_and_dispatch(cfg, handler).await {
                    log::debug!("[ZDO] rx dispatch error: {e:?}");
                }
            }
        }
        loop {
            if let Err(e) = self.poll_and_dispatch(cfg, handler).await {
                log::debug!("[ZDO] rx dispatch error: {e:?}");
            }
            delay.delay_ms(poll_interval_ms).await;
        }
    }
}
