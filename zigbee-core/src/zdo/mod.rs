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
use zigbee_types::sync::Signal;

use crate::apl::descriptors::node_descriptor::LogicalType;
use crate::aps::aib;
use crate::aps::aib::DeviceKeyPairDescriptor;
use crate::aps::aib::KeyAttribute;
use crate::aps::aib::LinkKeyType;
use crate::aps::aib::MAX_APS_BINDING_TABLE;
use crate::aps::apsde::ApsdeSapConfirm;
use crate::aps::apsde::ApsdeSapConfirmStatus;
use crate::aps::apsde::ApsdeSapIndication;
use crate::aps::apsde::ApsdeSapRequest;
use crate::aps::apsme::Apsme;
use crate::aps::frame::CommandFrame;
use crate::aps::frame::Frame;
use crate::aps::frame::command::Command;
use crate::aps::frame::command::TransportKey;
use crate::aps::types::TxOptions;
use crate::nwk::frame::command::network_status::NetworkStatusCode;
use crate::nwk::nib;
use crate::nwk::nib::NWK_BROADCAST_ADDRESS_MIN;
use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
use crate::nwk::nlme::KeepaliveMethod;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::Nlme;
use crate::nwk::nlme::management::NlmeJoinConfirm;
use crate::nwk::nlme::management::NlmeJoinRequest;
use crate::nwk::nlme::management::NlmeJoinStatus;
use crate::nwk::nlme::management::NlmeLeaveRequest;
use crate::nwk::nlme::management::NlmeLeaveStatus;
use crate::nwk::nlme::management::RejoinNetwork;
use crate::security::SecurityContext;

// number of MAC poll attempts when waiting for the Trust Center to deliver the
// network key (4.4.10); wide enough to cover the router -> TC -> router
// round-trip when the parent is not the coordinator
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
    // ZDP transaction sequence number (2.4.2), independent of the APS counter
    zdp_seq: AtomicU8,
    // signaled once the network key is installed; gates the receive loop
    joined: Event,
    // carries an inbound Node_Desc_rsp (2.4.4.2.3) to the procedure that
    // asked for it, e.g. the BDB TC link key exchange
    node_desc_rsp: Signal<descriptor::NodeDescRsp>,
    // set when a leave asked this device to rejoin (3.6.1.10.4 step 1)
    rejoin_requested: Event,
}

// Zigbee Device Profile identifier (endpoint 0)
const ZDP_PROFILE_ID: u16 = 0x0000;
const ZDO_ENDPOINT: u8 = 0x00;
// a ZDP response cluster is its request cluster with this bit set (2.4.4)
const ZDP_RESPONSE_CLUSTER_FLAG: u16 = 0x8000;

// an endpoint appears at most once in a MatchList (2.4.4.2.7 step 3.b)
const MAX_MATCHING_ENDPOINTS: usize = 16;

/// One inbound application-profile request handed to a
/// [`ClusterRequestHandler`].
#[derive(Debug, Clone, Copy)]
pub struct ClusterRequest<'a> {
    pub profile_id: u16,
    pub cluster_id: u16,
    pub src_endpoint: u8,
    pub dst_endpoint: u8,
    /// Whether the frame was addressed to this device alone; a broadcast must
    /// not draw a ZCL Default Response (ZCL 2.5.12.2).
    pub unicast: bool,
    pub asdu: &'a [u8],
}

/// Where a response goes, and how much of the output buffer it filled.
///
/// The APS layer defines no convention for how an application profile answers
/// a request (2.2.4.1.3), so the addressing belongs to whoever built the
/// response rather than to the layer sending it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterReply {
    pub cluster_id: u16,
    pub profile_id: u16,
    /// Endpoint on the requester the response is addressed to.
    pub dst_endpoint: u8,
    /// Endpoint on this device the response is sent from.
    pub src_endpoint: u8,
    /// Bytes written to the output buffer.
    pub len: usize,
}

impl ClusterReply {
    /// Answer on the request's own cluster and profile with the endpoints
    /// swapped, the convention the ZCL follows.
    pub const fn matching(request: &ClusterRequest<'_>, len: usize) -> Self {
        Self {
            cluster_id: request.cluster_id,
            profile_id: request.profile_id,
            dst_endpoint: request.src_endpoint,
            src_endpoint: request.dst_endpoint,
            len,
        }
    }
}

/// Handler for inbound application-profile (non-ZDP) requests.
///
/// Implemented by cluster servers (e.g. the ZCL Basic cluster) to answer
/// requests delivered by a receive loop. Takes `&self` so it can
/// be shared with the receive task.
pub trait ClusterRequestHandler {
    /// Build a response ASDU for an application-profile request, writing it
    /// into `out` and reporting where it goes, or `None` for no reply.
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply>;
}

/// Lets a handler be shared by reference, so a cluster server the application
/// also reads from can live outside the receive task.
impl<T: ClusterRequestHandler + ?Sized> ClusterRequestHandler for &T {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        (**self).handle(request, out)
    }
}

/// Chains two handlers: the first to produce a response wins. Lets an
/// application compose independent cluster servers (e.g. Basic-cluster reads
/// plus a generic reporting responder) into the single handler
/// a receive loop expects.
impl<A: ClusterRequestHandler, B: ClusterRequestHandler> ClusterRequestHandler for (A, B) {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.0
            .handle(request, out)
            .or_else(|| self.1.handle(request, out))
    }
}

/// Chains three handlers, left to right.
impl<A, B, C> ClusterRequestHandler for (A, B, C)
where
    A: ClusterRequestHandler,
    B: ClusterRequestHandler,
    C: ClusterRequestHandler,
{
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        (&self.0, (&self.1, &self.2)).handle(request, out)
    }
}

/// A ZDP response, which always travels between the two ZDO endpoints on the
/// Device Profile (2.4).
const fn zdp_reply(cluster_id: u16, len: usize) -> ClusterReply {
    ClusterReply {
        cluster_id,
        profile_id: ZDP_PROFILE_ID,
        dst_endpoint: ZDO_ENDPOINT,
        src_endpoint: ZDO_ENDPOINT,
        len,
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
            node_desc_rsp: Signal::new(),
            rejoin_requested: Event::new(),
        }
    }

    /// Forgets the joined network so the next boot re-commissions: clears the
    /// network address, the network key and the link keys negotiated with the
    /// Trust Center.
    ///
    /// With flash-backed information bases, flush the storage before
    /// resetting so this state survives the reboot.
    pub fn forget_network(&self) {
        let nib = nib::get_ref();
        nib.update_network_address(|value| *value = 0xffff);
        nib.update_security_material_set(|set| set.clear());
        self.reset_trust_center_link_keys();
    }

    /// Drops the link keys negotiated with the Trust Center so commissioning
    /// starts from the default TC link key again (4.4.10).
    ///
    /// Link keys are per-network: a stale key pair also carries its anti-replay
    /// counters, which reject the Transport-Key of a new join.
    pub fn reset_trust_center_link_keys(&self) {
        let aib = aib::get_ref();
        aib.update_device_key_pair_set(|set| set.clear());
        aib.update_trust_center_address(|value| *value = IeeeAddress(0xffff_ffff_ffff_ffff));
    }

    /// Access the owned NWK management entity (used by BDB during
    /// commissioning).
    pub fn nlme(&self) -> &Nlme<M> {
        &self.nlme
    }

    // next ZDP transaction sequence number (2.4.2), wrapping
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

    /// APSDE-DATA.request (2.2.4.1.1).
    ///
    /// Builds an APS data frame for the given destination + endpoint +
    /// cluster + profile and hands it to the NWK layer for encryption with
    /// the active network key and transmission via the parent.
    ///
    /// [`DstAddrMode::Network`] addresses a destination directly;
    /// [`DstAddrMode::None`] resolves the destination from the binding table
    /// (2.2.8.4.1). Group and extended addressing return
    /// [`ApsdeSapConfirmStatus::Unsupported`].
    ///
    /// With [`TxOptions::Acknowledged`] the frame requests an APS
    /// acknowledgement and is retransmitted until one arrives or
    /// `apscMaxFrameRetries` is exhausted (2.2.8.4.4); `delay` paces that
    /// wait.
    pub async fn data_request(
        &self,
        request: ApsdeSapRequest<'_>,
        delay: &mut impl DelayNs,
    ) -> ApsdeSapConfirm {
        use crate::aps::types::Address;
        use crate::aps::types::DstAddrMode;

        let status = match (request.dst_addr_mode, request.dst_address) {
            (DstAddrMode::Network, Address::Network(short)) => {
                self.send_asdu(ShortAddress(short), &request, request.dst_endpoint, delay)
                    .await
            }
            (DstAddrMode::None, _) => self.send_to_bindings(&request, delay).await,
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

    /// Transmit one ASDU to a resolved destination, honoring the request's
    /// acknowledgement option (2.2.8.4.3).
    async fn send_asdu(
        &self,
        destination: ShortAddress,
        request: &ApsdeSapRequest<'_>,
        dst_endpoint: u8,
        delay: &mut impl DelayNs,
    ) -> ApsdeSapConfirmStatus {
        let result = if request.tx_options == TxOptions::Acknowledged {
            self.apsme
                .unicast_data_acked(
                    &self.nlme,
                    destination,
                    dst_endpoint,
                    request.cluster_id,
                    request.profile_id,
                    request.src_endpoint.value,
                    request.asdu,
                    delay,
                )
                .await
        } else {
            self.apsme
                .unicast_data(
                    &self.nlme,
                    destination,
                    dst_endpoint,
                    request.cluster_id,
                    request.profile_id,
                    request.src_endpoint.value,
                    request.asdu,
                )
                .await
        };

        match result {
            Ok(()) => ApsdeSapConfirmStatus::Success,
            Err(NetworkError::FrameTooLong) => ApsdeSapConfirmStatus::AsduTooLong,
            Err(NetworkError::SecurityError(_)) => ApsdeSapConfirmStatus::SecurityFail,
            Err(NetworkError::NotJoined) => ApsdeSapConfirmStatus::NoShortAddress,
            Err(e) => {
                log::debug!("[ZDO] tx failed: {e:?}");
                ApsdeSapConfirmStatus::NoAck
            }
        }
    }

    /// Transmit an ASDU to every binding table entry matching the request's
    /// source endpoint and cluster (2.2.8.4.1).
    async fn send_to_bindings(
        &self,
        request: &ApsdeSapRequest<'_>,
        delay: &mut impl DelayNs,
    ) -> ApsdeSapConfirmStatus {
        // the destinations are collected first: the AIB guard must not be held
        // across a transmission
        let mut destinations: heapless::Vec<(ShortAddress, u8), MAX_APS_BINDING_TABLE> =
            heapless::Vec::new();
        let mut unresolved = false;
        let mut group_binding = false;

        for binding in aib::get_ref()
            .binding_table()
            .iter()
            .filter(|b| b.matches(request.src_endpoint.value, request.cluster_id))
        {
            let (Some(ieee), Some(dst_endpoint)) = (binding.dst_ieee, binding.dst_endpoint) else {
                // group addressing needs the group table (2.2.8.4.1)
                group_binding = true;
                continue;
            };
            match self.resolve_binding_address(ieee) {
                Some(short) => {
                    let _ = destinations.push((short, dst_endpoint));
                }
                None => unresolved = true,
            }
        }

        if destinations.is_empty() {
            return match (group_binding, unresolved) {
                (_, true) => ApsdeSapConfirmStatus::NoShortAddress,
                (true, _) => ApsdeSapConfirmStatus::Unsupported,
                _ => ApsdeSapConfirmStatus::NoBoundDevice,
            };
        }

        let mut status = ApsdeSapConfirmStatus::Success;
        for (destination, dst_endpoint) in destinations {
            let result = self
                .send_asdu(destination, request, dst_endpoint, delay)
                .await;
            if result != ApsdeSapConfirmStatus::Success && status == ApsdeSapConfirmStatus::Success
            {
                status = result;
            }
        }
        status
    }

    /// Resolve a binding's IEEE address to a network address.
    ///
    /// An end device only learns addresses from received frames, so the trust
    /// center — the usual binding destination — is resolved from the AIB
    /// instead. This stack does not issue a NWK_addr_req.
    fn resolve_binding_address(&self, ieee: IeeeAddress) -> Option<ShortAddress> {
        if ieee == *aib::get_ref().trust_center_address() {
            return Some(ShortAddress::COORDINATOR);
        }
        self.nlme.short_address_for_ieee(ieee)
    }

    /// 2.1.3.1 - Device Discovery
    /// is the process whereby a Zigbee device can discover other Zigbee
    /// devices.
    pub fn start_device_discovery(&self) {
        match self.config.device_discovery_type {
            // TODO: unicast IEEE address request, then wait for the response
            config::DiscoveryType::IEEE => todo!(),
            // TODO: broadcast NWK address request with the known IEEE address, then wait for the
            // response
            config::DiscoveryType::NWK => todo!(),
        }
    }

    /// Service discovery (2.1.3.2): discovers the capabilities of another
    /// device.
    pub fn start_service_discovery(&self) {}

    /// Broadcast a ZDO Device_annce (2.4.3.1.11).
    pub async fn device_annce(&self, annce: device_annce::DeviceAnnce) -> Result<(), NetworkError> {
        device_annce::broadcast(&self.nlme, &self.apsme, self.next_zdp_seq(), annce).await
    }

    /// Broadcast a ZDO Device_annce for this device (2.4.3.1.11), announcing
    /// the addresses it holds after a join or rejoin.
    pub async fn announce_self(&self) -> Result<(), NetworkError> {
        let nib = nib::get_ref();
        let annce = device_annce::DeviceAnnce {
            nwk_addr: ShortAddress(*nib.network_address()),
            ieee_addr: *nib.ieee_address(),
            capability: *nib.capability_information(),
        };
        self.device_annce(annce).await
    }

    /// Network Manager (2.5.2.4): rejoin the network after communication with
    /// it was lost.
    ///
    /// A Secure Rejoin is attempted first — it secures the Rejoin Request with
    /// the active network key and completes in a single exchange. If that
    /// fails, typically because a network key update was missed, a Trust
    /// Center rejoin follows: the request goes out unsecured and the current
    /// network key is obtained from the Trust Center (3.6.10.10, 4.6.3.3.2).
    /// Both keep the Trust Center link key, so neither needs re-commissioning.
    ///
    /// The remembered parent is addressed first; if it does not answer,
    /// `channels` are scanned for another router of the same network
    /// (3.6.1.4.2). On success the end-device timeout is renegotiated
    /// (3.6.10.2) and the device announces itself again.
    ///
    /// `extended_pan_id` names the network to return to: a leave zeroes
    /// `nwkExtendedPANId` (3.6.1.10.1), so it cannot always be read back from
    /// the NIB.
    pub async fn rejoin(
        &self,
        extended_pan_id: IeeeAddress,
        channels: core::ops::Range<u8>,
        scan_duration: u8,
    ) -> Result<NlmeJoinConfirm, NetworkError> {
        let capability_information = *nib::get_ref().capability_information();
        let request = |security_enabled| NlmeJoinRequest {
            extended_pan_id,
            rejoin_network: RejoinNetwork::NwkRejoin,
            capability_information,
            security_enabled,
            scan_channels: channels.clone(),
            scan_duration,
        };

        log::debug!("[ZDO] secure rejoin, EPID={extended_pan_id:?}");
        let confirm = self.nlme.join(request(true)).await;
        if confirm.status == NlmeJoinStatus::Success {
            self.finish_rejoin().await?;
            return Ok(confirm);
        }

        log::warn!(
            "[ZDO] secure rejoin failed ({:?}), falling back to trust center rejoin",
            confirm.status
        );
        let confirm = self.nlme.join(request(false)).await;
        if confirm.status != NlmeJoinStatus::Success {
            log::warn!("[ZDO] trust center rejoin failed ({:?})", confirm.status);
            return Ok(confirm);
        }

        // 4.6.3.3.2: the unsecured rejoin leaves the device without a valid
        // network key until the Trust Center transports the current one
        self.poll_transport_key().await?;
        self.finish_rejoin().await?;
        Ok(confirm)
    }

    // pick up where a successful rejoin leaves off: the receive loop was
    // parked by the leave or the link failure, the timeout is renegotiated
    // with the (possibly new) parent, and the network is told which addresses
    // this device holds now
    async fn finish_rejoin(&self) -> Result<(), NetworkError> {
        self.mark_rejoined();
        // 3.6.10.2: renegotiated after every rejoin, even with the same parent
        let _ = self.nlme.negotiate_end_device_timeout().await;
        self.announce_self().await
    }

    /// Security Manager: poll for a Transport-Key command and install the
    /// network key and Trust Center link key entry (4.4.10).
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
                self.mark_joined(true);
            }
            // only the network key completes the join (4.4.10); anything
            // else here is a stale frame — fail instead of proceeding keyless
            TransportKey::ApplicationLinkKey(_)
            | TransportKey::TrustCenterLinkKey(_)
            | TransportKey::Reserved(_) => return Err(NetworkError::NoTransportKey),
        }

        Ok(())
    }

    /// Security Manager: build and send an APS command frame (4.4).
    ///
    /// Delegates to APSME which owns `apsCounter` (4.4.11). When
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

    // open or close the join gate, keeping the APS layer's view in sync
    fn mark_joined(&self, joined: bool) {
        if joined {
            self.joined.signal();
        } else {
            self.joined.reset();
        }
        self.apsme.set_joined(joined);
    }

    // close the receive loop's join gate on NLME-LEAVE.indication with a NULL
    // device address (3.2.2.19); parks the loop until re-commissioned or
    // rejoined
    fn close_gate_on_leave_indication(&self) {
        let Some(indication) = self.nlme.take_leave_indication() else {
            return;
        };
        if indication.device_address.is_some() {
            return;
        }
        log::info!(
            "[ZDO] removed from the network (rejoin={})",
            indication.rejoin
        );
        self.mark_joined(false);
        // 3.6.1.10.4 step 1: a leave with the rejoin flag keeps the NIB and
        // asks the device back onto the network — the higher layer drives it
        if indication.rejoin {
            self.rejoin_requested.signal();
        }
    }

    /// Whether a leave asked this device to rejoin the network (3.6.1.10.4).
    ///
    /// Consumes the request: the receive loop is parked until the higher layer
    /// rejoins and calls [`Self::mark_rejoined`].
    pub fn take_rejoin_request(&self) -> bool {
        if !self.rejoin_requested.is_set() {
            return false;
        }
        self.rejoin_requested.reset();
        true
    }

    /// Wait until a leave asks this device to rejoin the network
    /// (3.6.1.10.4).
    pub async fn wait_rejoin_request(&self) {
        self.rejoin_requested.wait().await;
    }

    /// Re-open the receive loop's join gate after the higher layer rejoined
    /// the network.
    ///
    /// A leave with the rejoin flag, or a lost parent link, parks the loop
    /// until the device is back on the network.
    pub fn mark_rejoined(&self) {
        self.mark_joined(true);
    }

    /// Arm the TC link key exchange (BDB 10.2.5): key-transport commands
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

    /// Wait until the Trust Center delivered a new link key (4.4.10) and
    /// report whether it was accepted.
    ///
    /// The receive loop installs the key as unverified into the AIB before
    /// signaling. `false` means the transported key was rejected — it was the
    /// one already held (BDB 10.2.5 step 9) — and the exchange has failed.
    /// Bound the wait with an executor timeout.
    pub async fn wait_tc_link_key_received(&self) -> bool {
        self.apsme.tc_key_received.wait().await
    }

    /// Send a ZDO Node_Desc_req (2.4.3.1.3) asking `destination` for its own
    /// node descriptor, discarding a response left over from an earlier
    /// request.
    ///
    /// Await the answer with [`Self::wait_node_desc_rsp`]; the receive loop
    /// must be running to deliver it.
    pub async fn node_desc_req(&self, destination: ShortAddress) -> Result<(), NetworkError> {
        self.node_desc_rsp.reset();

        let mut asdu = [0u8; 3];
        let len = descriptor::node_desc_req(self.next_zdp_seq(), destination.0, &mut asdu)?;
        self.apsme
            .unicast_data(
                &self.nlme,
                destination,
                ZDO_ENDPOINT,
                descriptor::NODE_DESC_REQ,
                ZDP_PROFILE_ID,
                ZDO_ENDPOINT,
                &asdu[..len],
            )
            .await
    }

    /// Wait for a Node_Desc_rsp (2.4.4.2.3).
    ///
    /// Bound the wait with an executor timeout.
    pub async fn wait_node_desc_rsp(&self) -> descriptor::NodeDescRsp {
        self.node_desc_rsp.wait().await
    }

    /// Leave the network and forget it: announces the departure (3.6.1.10.1),
    /// clears the NIB network attributes and the Trust Center link keys, and
    /// parks the receive loop until the device is commissioned again.
    ///
    /// With flash-backed information bases, flush the storage afterwards so
    /// the reset survives a reboot.
    pub async fn leave_network(&self, rejoin: bool) -> NlmeLeaveStatus {
        let confirm = self
            .nlme
            .leave(NlmeLeaveRequest {
                device_address: None,
                remove_children: false,
                rejoin,
            })
            .await;
        self.mark_joined(false);
        self.reset_trust_center_link_keys();
        confirm.status
    }

    /// Wait until the Trust Center answered the key verification (4.4.9)
    /// and return the Confirm-Key status (0x00 = success, key marked
    /// verified).
    ///
    /// Bound the wait with an executor timeout.
    pub async fn wait_tc_link_key_verified(&self) -> u8 {
        self.apsme.tc_key_verified.wait().await
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

    // answer one dispatched APSDE-DATA.indication, if any
    async fn dispatch(
        &self,
        indication: Option<ApsdeSapIndication<'_>>,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        handler: &impl ClusterRequestHandler,
    ) -> Result<(), NetworkError> {
        use crate::aps::types::Address;

        let mut out = [0u8; 128];

        // a NWK Leave command consumed by the NWK layer surfaces here as an
        // NLME-LEAVE.indication rather than as a dispatchable frame
        self.close_gate_on_leave_indication();

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

        // deferred until after the response below has actually gone out: the
        // leave procedure clears the NWK security material this reply is
        // encrypted with, so executing it first would leave the reply
        // unsendable.
        let mut leave_after_reply = None;

        // a broadcast is answered differently from a unicast, both for
        // Match_Desc_req (2.4.4.2.7) and Mgmt_Leave_req (3.6.1.10.3 step 1)
        let nwk_dst = match indication.dst_address {
            Address::Network(dst) => dst,
            _ => *self.nlme.nib().network_address(),
        };

        let response = if profile == ZDP_PROFILE_ID && cluster == descriptor::MGMT_LEAVE_REQ {
            self.build_mgmt_leave_rsp(indication.asdu, ShortAddress(nwk_dst), &mut out)
                .map(|(len, request)| {
                    leave_after_reply = request;
                    zdp_reply(descriptor::MGMT_LEAVE_RSP, len)
                })
        } else if profile == ZDP_PROFILE_ID && cluster == descriptor::NODE_DESC_RSP {
            // answer to our own Node_Desc_req: hand it to whoever is waiting
            if let Some(response) = descriptor::parse_node_desc_rsp(indication.asdu) {
                self.node_desc_rsp.signal(response);
            }
            None
        } else if profile == ZDP_PROFILE_ID {
            self.build_zdp_response(cluster, indication.asdu, nwk_dst, cfg, &mut out)
                .map(|(rsp_cluster, len)| zdp_reply(rsp_cluster, len))
        } else {
            let request = ClusterRequest {
                profile_id: profile,
                cluster_id: cluster,
                src_endpoint,
                dst_endpoint,
                unicast: nwk_dst < NWK_BROADCAST_ADDRESS_MIN,
                asdu: indication.asdu,
            };
            // the handler owns the response addressing: this layer knows
            // nothing about how the profile answers
            handler.handle(&request, &mut out)
        };

        if let Some(reply) = response {
            log::debug!(
                "[ZDO] tx response cluster={:#06x} ({} bytes) to {src:#06x}",
                reply.cluster_id,
                reply.len
            );
            self.apsme
                .unicast_data(
                    &self.nlme,
                    ShortAddress(src),
                    reply.dst_endpoint,
                    reply.cluster_id,
                    reply.profile_id,
                    reply.src_endpoint,
                    &out[..reply.len],
                )
                .await?;
        } else {
            log::trace!("[ZDO] no response for cluster {cluster:#06x}");
        }

        if let Some(request) = leave_after_reply {
            let confirm = self.nlme.leave(request).await;
            log::info!(
                "[ZDO] leaving network after Mgmt_Leave_req: {:?}",
                confirm.status
            );
            // a self-leave yields a confirm, not an indication (3.2.2.18), so
            // close the gate here
            self.mark_joined(false);
        }
        Ok(())
    }

    // answer a Match_Desc_req (2.4.4.2.7) about this device: a broadcast about
    // another device (step 2.a) or without a match (step 5) stays silent. a
    // request about another device is answered with INV_REQUESTTYPE by an end
    // device (step 2.b) and with DEVICE_NOT_FOUND by a router, which keeps no
    // cached simple descriptors for its children (step 4.b)
    fn build_match_desc_rsp(
        &self,
        asdu: &[u8],
        nwk_dst: u16,
        nwk_addr: u16,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        out: &mut [u8],
    ) -> Option<(u16, usize)> {
        let request = descriptor::parse_match_desc_req(asdu)?;
        let broadcast = nwk_dst >= NWK_BROADCAST_ADDRESS_MIN;
        let about_us = request.nwk_addr_of_interest == nwk_addr
            || request.nwk_addr_of_interest >= NWK_BROADCAST_ADDRESS_MIN;

        if !about_us {
            // an end device answers nothing about another device (step 2.a);
            // a router still reports DEVICE_NOT_FOUND (step 4.b)
            if broadcast && cfg.node.logical_type == LogicalType::EndDevice {
                return None;
            }
            let error = Self::discovery_error(cfg);
            return Some((
                descriptor::MATCH_DESC_RSP,
                descriptor::match_desc_rsp_error(request.seq, error, out).ok()?,
            ));
        }

        let mut matches = [0u8; MAX_MATCHING_ENDPOINTS];
        let count = cfg.matching_endpoints(&request, &mut matches);
        if count == 0 && broadcast {
            return None;
        }

        Some((
            descriptor::MATCH_DESC_RSP,
            descriptor::match_desc_rsp(request.seq, nwk_addr, &matches[..count], out).ok()?,
        ))
    }

    // build a Mgmt_Leave_rsp payload (2.4.4.4.5); returns its length and the
    // NLME-LEAVE.request to issue once the response has been sent. only a
    // request naming this device (or the NULL address) is honored — removing
    // a child requires acting as its parent, which this stack does not support
    fn build_mgmt_leave_rsp(
        &self,
        asdu: &[u8],
        destination: ShortAddress,
        out: &mut [u8],
    ) -> Option<(usize, Option<NlmeLeaveRequest>)> {
        let request = descriptor::parse_mgmt_leave_req(asdu)?;

        let own_ieee = *self.nlme.nib().ieee_address();
        let is_self = request.device_address.is_none_or(|addr| addr == own_ieee);

        let (status, leave) = if !is_self {
            (NlmeLeaveStatus::UnknownDevice, None)
        } else if !self.nlme.accepts_leave_request(destination, true) {
            (NlmeLeaveStatus::NotAuthorized, None)
        } else {
            (
                NlmeLeaveStatus::Success,
                Some(NlmeLeaveRequest {
                    device_address: None,
                    remove_children: request.remove_children,
                    rejoin: request.rejoin,
                }),
            )
        };

        let len =
            descriptor::mgmt_leave_rsp(request.seq, descriptor::leave_status(status), out).ok()?;
        Some((len, leave))
    }

    // apply a ZDP Bind_req or Unbind_req to the binding table (2.4.3.2.2),
    // returning the status for the response. Only bindings sourced at this
    // device are accepted: this stack does not keep a binding table cache on
    // behalf of others
    fn apply_bind_req(
        &self,
        asdu: &[u8],
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        bind: bool,
    ) -> descriptor::BindStatus {
        use crate::aps::apsme::ApsmeSap;
        use crate::aps::apsme::basemgt::ApsmeBindRequest;
        use crate::aps::apsme::basemgt::ApsmeBindRequestStatus;
        use crate::aps::apsme::basemgt::ApsmeUnbindRequest;
        use crate::aps::apsme::basemgt::ApsmeUnbindRequestStatus;
        use crate::aps::binding::BindingAddrMode;
        use crate::aps::types::Address;
        use crate::aps::types::SrcEndpoint;
        use crate::zdo::descriptor::BindReqDestination;
        use crate::zdo::descriptor::BindStatus;

        let Some(request) = descriptor::parse_bind_req(asdu) else {
            return BindStatus::NotSupported;
        };
        if request.src_address != *self.nlme.nib().ieee_address() {
            return BindStatus::NotSupported;
        }
        let Ok(src_endpoint) = SrcEndpoint::new(request.src_endpoint) else {
            return BindStatus::InvalidEndpoint;
        };
        if !cfg.supports_endpoint(request.src_endpoint) {
            return BindStatus::InvalidEndpoint;
        }

        let (dst_addr_mode, dst_address, dst_endpoint) = match request.destination {
            BindReqDestination::Device { ieee, endpoint } => {
                (BindingAddrMode::Device, Address::Extended(ieee.0), endpoint)
            }
            BindReqDestination::Group(group) => (BindingAddrMode::Group, Address::Group(group), 0),
        };

        let bind_request = ApsmeBindRequest {
            src_address: Address::Extended(request.src_address.0),
            src_endpoint,
            cluster_id: request.cluster_id,
            dst_addr_mode,
            dst_address,
            dst_endpoint,
        };

        if bind {
            return match self.apsme.bind_request(bind_request).status {
                ApsmeBindRequestStatus::Success => BindStatus::Success,
                ApsmeBindRequestStatus::TableFull => BindStatus::TableFull,
                _ => BindStatus::NotSupported,
            };
        }

        let unbind_request = ApsmeUnbindRequest {
            src_address: bind_request.src_address,
            src_endpoint: bind_request.src_endpoint,
            cluster_id: bind_request.cluster_id,
            dst_addr_mode: bind_request.dst_addr_mode,
            dst_address: bind_request.dst_address,
            dst_endpoint: bind_request.dst_endpoint,
        };
        match self.apsme.unbind_request(unbind_request).status {
            ApsmeUnbindRequestStatus::Success => BindStatus::Success,
            ApsmeUnbindRequestStatus::InvalidBinding => BindStatus::NoEntry,
            ApsmeUnbindRequestStatus::IllegalRequest => BindStatus::NotSupported,
        }
    }

    // the status a discovery request about another device is answered with
    // (2.4.4.2.7 steps 2.b/4.b): an end device answers for itself only, a
    // router keeps no cached descriptors for its children
    fn discovery_error(cfg: &descriptor::DeviceDescriptorConfig<'_>) -> descriptor::DiscoveryError {
        if cfg.node.logical_type == LogicalType::EndDevice {
            descriptor::DiscoveryError::InvalidRequestType
        } else {
            descriptor::DiscoveryError::DeviceNotFound
        }
    }

    // answer an address request (2.4.4.2.1/2.4.4.2.2). only a request naming
    // this device is answered with its addresses — replying to every lookup
    // would poison the requester's address map. a broadcast that names another
    // device stays silent, a unicast reports DEVICE_NOT_FOUND
    fn build_addr_rsp(
        &self,
        cluster: u16,
        asdu: &[u8],
        nwk_dst: u16,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        out: &mut [u8],
    ) -> Option<(u16, usize)> {
        let nwk_addr = *self.nlme.nib().network_address();
        let ieee_addr = *self.nlme.nib().ieee_address();

        // the error paths echo the requested addresses, leaving the one this
        // device cannot resolve unknown
        let (rsp_cluster, seq, request_type, about_us, asked_ieee, asked_nwk) =
            if cluster == descriptor::NWK_ADDR_REQ {
                let request = descriptor::parse_nwk_addr_req(asdu)?;
                (
                    descriptor::NWK_ADDR_RSP,
                    request.seq,
                    request.request_type,
                    request.ieee_addr == ieee_addr,
                    request.ieee_addr,
                    ShortAddress::default().0,
                )
            } else {
                let request = descriptor::parse_ieee_addr_req(asdu)?;
                (
                    descriptor::IEEE_ADDR_RSP,
                    request.seq,
                    request.request_type,
                    request.nwk_addr_of_interest == nwk_addr,
                    IeeeAddress(u64::MAX),
                    request.nwk_addr_of_interest,
                )
            };

        if !about_us {
            if nwk_dst >= NWK_BROADCAST_ADDRESS_MIN {
                return None;
            }
            let error = Self::discovery_error(cfg);
            let len = descriptor::addr_rsp_error(seq, error, asked_ieee, asked_nwk, out).ok()?;
            return Some((rsp_cluster, len));
        }

        let extended = match request_type {
            descriptor::ADDR_REQUEST_SINGLE => false,
            descriptor::ADDR_REQUEST_EXTENDED => true,
            _ => {
                let len = descriptor::addr_rsp_error(
                    seq,
                    descriptor::DiscoveryError::InvalidRequestType,
                    ieee_addr,
                    nwk_addr,
                    out,
                )
                .ok()?;
                return Some((rsp_cluster, len));
            }
        };

        let len = descriptor::addr_rsp(seq, ieee_addr, nwk_addr, extended, out).ok()?;
        Some((rsp_cluster, len))
    }

    // build the ZDP `*_rsp` payload for a discovery request, echoing the
    // request's transaction sequence number; None for unsupported requests
    fn build_zdp_response(
        &self,
        cluster: u16,
        asdu: &[u8],
        nwk_dst: u16,
        cfg: &descriptor::DeviceDescriptorConfig<'_>,
        out: &mut [u8],
    ) -> Option<(u16, usize)> {
        let seq = *asdu.first()?;
        let nwk_addr = *self.nlme.nib().network_address();

        // a descriptor request naming another device is not answered with our
        // own descriptors (2.4.4.2.3-2.4.4.2.6)
        let descriptor_request = matches!(
            cluster,
            descriptor::NODE_DESC_REQ
                | descriptor::POWER_DESC_REQ
                | descriptor::ACTIVE_EP_REQ
                | descriptor::SIMPLE_DESC_REQ
        );
        if descriptor_request {
            let addr_of_interest = descriptor::parse_addr_of_interest(asdu)?;
            if addr_of_interest != nwk_addr {
                if nwk_dst >= NWK_BROADCAST_ADDRESS_MIN {
                    return None;
                }
                let error = Self::discovery_error(cfg);
                let rsp_cluster = cluster | ZDP_RESPONSE_CLUSTER_FLAG;
                // Simple_Desc_rsp and Active_EP_rsp keep their count field on
                // the error paths (2.4.4.2.5/2.4.4.2.6)
                let len = match cluster {
                    descriptor::SIMPLE_DESC_REQ => {
                        descriptor::simple_desc_rsp_error(seq, error, addr_of_interest, out).ok()?
                    }
                    descriptor::ACTIVE_EP_REQ => {
                        descriptor::active_ep_rsp_error(seq, error, addr_of_interest, out).ok()?
                    }
                    _ => {
                        descriptor::descriptor_rsp_error(seq, error, addr_of_interest, out).ok()?
                    }
                };
                return Some((rsp_cluster, len));
            }
        }

        let result = match cluster {
            descriptor::NODE_DESC_REQ => (
                descriptor::NODE_DESC_RSP,
                cfg.node_desc_rsp(seq, nwk_addr, out).ok()?,
            ),
            descriptor::POWER_DESC_REQ => (
                descriptor::POWER_DESC_RSP,
                cfg.power_desc_rsp(seq, nwk_addr, out).ok()?,
            ),
            descriptor::MATCH_DESC_REQ => {
                return self.build_match_desc_rsp(asdu, nwk_dst, nwk_addr, cfg, out);
            }
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
            descriptor::IEEE_ADDR_REQ | descriptor::NWK_ADDR_REQ => {
                return self.build_addr_rsp(cluster, asdu, nwk_dst, cfg, out);
            }
            descriptor::BIND_REQ => (
                descriptor::BIND_RSP,
                descriptor::bind_rsp(seq, self.apply_bind_req(asdu, cfg, true), out).ok()?,
            ),
            descriptor::UNBIND_REQ => (
                descriptor::UNBIND_RSP,
                descriptor::bind_rsp(seq, self.apply_bind_req(asdu, cfg, false), out).ok()?,
            ),
            _ => return None,
        };
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use zigbee_mac::Address;
    use zigbee_mac::mlme::AssociationResponse;
    use zigbee_mac::mlme::MacConfig;
    use zigbee_mac::mlme::MacError;
    use zigbee_mac::mlme::ScanResult;
    use zigbee_mac::mlme::ScanType;

    use super::*;
    // NIB and AIB are process-wide singletons
    use crate::TEST_MUTEX;
    use crate::apl::descriptors::node_power_descriptor::CurrentPowerMode;
    use crate::apl::descriptors::node_power_descriptor::CurrentPowerSourceLevel;
    use crate::apl::descriptors::node_power_descriptor::PowerSource;
    use crate::zdo::descriptor::DeviceDescriptorConfig;
    use crate::zdo::descriptor::EndpointDescriptor;
    use crate::zdo::descriptor::NodeDescriptorConfig;
    use crate::zdo::descriptor::PowerDescriptorConfig;

    // requests naming anything else take the error paths under test
    const OUR_NWK_ADDR: u16 = 0x1234;
    const OTHER_NWK_ADDR: u16 = 0x9999;

    mockall::mock! {
        Mlme {}
        impl Mlme for Mlme {
            fn ieee_address(&self) -> IeeeAddress;
            async fn configure(&self, config: MacConfig);
            async fn scan_network(
                &self,
                ty: ScanType,
                channels: core::ops::Range<u8>,
                duration: u8,
            ) -> Result<ScanResult, MacError>;
            async fn associate(
                &self,
                channel: u8,
                dest: Address,
                capabilities: zigbee_mac::CapabilityInformation,
            ) -> Result<AssociationResponse, MacError>;
            async fn poll_data(
                &self,
                coord_address: Address,
                buf: &mut [u8],
            ) -> Result<(usize, u8), MacError>;
            async fn receive(
                &self,
                buf: &mut [u8],
            ) -> Result<(usize, u8), MacError>;
            async fn transmit_data(
                &self,
                dest: Address,
                payload: &[u8],
            ) -> Result<(), MacError>;
        }
    }

    // mirrors the fixtures in descriptor::tests
    const NODE: NodeDescriptorConfig = NodeDescriptorConfig {
        logical_type: LogicalType::EndDevice,
        complex_descriptor_available: false,
        user_descriptor_available: false,
        frequency_band: 0x08,
        mac_capability_flags: 0x88,
        manufacturer_code: 0x1037,
        maximum_buffer_size: 80,
        maximum_incoming_transfer_size: 128,
        server_mask: 0,
        maximum_outgoing_transfer_size: 128,
        descriptor_capability_field: 0,
    };

    const POWER: PowerDescriptorConfig = PowerDescriptorConfig {
        current_power_mode: CurrentPowerMode::Synchronized,
        available_power_sources: &[PowerSource::ConstantMainPower],
        current_power_source: PowerSource::ConstantMainPower,
        current_power_source_level: CurrentPowerSourceLevel::Full,
    };

    const ENDPOINTS: [EndpointDescriptor; 1] = [EndpointDescriptor {
        endpoint: 1,
        profile_id: 0x0104,
        device_id: 0x0302,
        device_version: 1,
        input_clusters: &[0x0000, 0x0402],
        output_clusters: &[0x0019],
    }];

    fn cfg(logical_type: LogicalType) -> DeviceDescriptorConfig<'static> {
        DeviceDescriptorConfig {
            node: NodeDescriptorConfig {
                logical_type,
                ..NODE
            },
            power: POWER,
            endpoints: &ENDPOINTS,
        }
    }

    fn make_device(logical_type: LogicalType) -> ZigbeeDevice<MockMlme> {
        let mut mac = MockMlme::new();
        mac.expect_ieee_address()
            .return_const(IeeeAddress(0xa4c1_0000_0000_0001));
        let nlme = Nlme::new(mac);
        let config = Config {
            device_type: logical_type,
            ..Config::default()
        };
        ZigbeeDevice::new(config, nlme)
    }

    // 2.4.3.1.3-2.4.3.1.6: seq, NWKAddrOfInterest
    fn descriptor_req(seq: u8, addr_of_interest: u16) -> [u8; 3] {
        let [lo, hi] = addr_of_interest.to_le_bytes();
        [seq, lo, hi]
    }

    // 2.4.4.2.3-2.4.4.2.6: each error response carries the fields of its own
    // figure, count fields included and set to zero
    #[test]
    fn discovery_error_responses_carry_their_count_fields() {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        nib::try_init();
        nib::reset();
        aib::try_init();
        let mut out = [0u8; 32];

        let router = make_device(LogicalType::Router);
        // Nlme::new stamped the IEEE address, the short one is ours to set
        nib::get_ref().update_network_address(|value| *value = OUR_NWK_ADDR);

        // 2.4.4.2.3: seq, status, NWKAddrOfInterest and no count field
        let (cluster, len) = router
            .build_zdp_response(
                descriptor::NODE_DESC_REQ,
                &descriptor_req(0x07, OTHER_NWK_ADDR),
                OUR_NWK_ADDR,
                &cfg(LogicalType::Router),
                &mut out,
            )
            .expect("a unicast Node_Desc_req naming another device is answered");
        assert_eq!(cluster, descriptor::NODE_DESC_RSP);
        assert_eq!(&out[..len], &[0x07, 0x81, 0x99, 0x99]);

        // 2.4.4.2.4: same shape
        let (cluster, len) = router
            .build_zdp_response(
                descriptor::POWER_DESC_REQ,
                &descriptor_req(0x08, OTHER_NWK_ADDR),
                OUR_NWK_ADDR,
                &cfg(LogicalType::Router),
                &mut out,
            )
            .expect("a unicast Power_Desc_req naming another device is answered");
        assert_eq!(cluster, descriptor::POWER_DESC_RSP);
        assert_eq!(&out[..len], &[0x08, 0x81, 0x99, 0x99]);

        // 2.4.4.2.5: a zero Length replaces the descriptor
        let (cluster, len) = router
            .build_zdp_response(
                descriptor::SIMPLE_DESC_REQ,
                &descriptor_req(0x09, OTHER_NWK_ADDR),
                OUR_NWK_ADDR,
                &cfg(LogicalType::Router),
                &mut out,
            )
            .expect("a unicast Simple_Desc_req naming another device is answered");
        assert_eq!(cluster, descriptor::SIMPLE_DESC_RSP);
        assert_eq!(&out[..len], &[0x09, 0x81, 0x99, 0x99, 0x00]);

        // 2.4.4.2.6: ActiveEPCount is set to 0 and only ActiveEPList omitted
        let (cluster, len) = router
            .build_zdp_response(
                descriptor::ACTIVE_EP_REQ,
                &descriptor_req(0x0a, OTHER_NWK_ADDR),
                OUR_NWK_ADDR,
                &cfg(LogicalType::Router),
                &mut out,
            )
            .expect("a unicast Active_EP_req naming another device is answered");
        assert_eq!(cluster, descriptor::ACTIVE_EP_RSP);
        assert_eq!(
            &out[..len],
            &[0x0a, 0x81, 0x99, 0x99, 0x00],
            "Active_EP_rsp must carry ActiveEPCount = 0 (2.4.4.2.6); a four-byte \
             response is short by that field and reads as malformed"
        );
    }

    // 2.4.4.2.6 keeps the count on the INV_REQUESTTYPE path too
    #[test]
    fn active_ep_rsp_invalid_request_type_carries_the_count() {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        nib::try_init();
        nib::reset();
        aib::try_init();
        let mut out = [0u8; 32];

        let end_device = make_device(LogicalType::EndDevice);
        nib::get_ref().update_network_address(|value| *value = OUR_NWK_ADDR);

        let (cluster, len) = end_device
            .build_zdp_response(
                descriptor::ACTIVE_EP_REQ,
                &descriptor_req(0x0b, OTHER_NWK_ADDR),
                OUR_NWK_ADDR,
                &cfg(LogicalType::EndDevice),
                &mut out,
            )
            .expect("a unicast Active_EP_req naming another device is answered");
        assert_eq!(cluster, descriptor::ACTIVE_EP_RSP);
        assert_eq!(
            &out[..len],
            &[0x0b, 0x80, 0x99, 0x99, 0x00],
            "Active_EP_rsp must carry ActiveEPCount = 0 (2.4.4.2.6)"
        );
    }
}
