//! Network Management Entity
//!
//! The NLME shall provide a management service to allow an application to
//! interact with the stack.
//!
//! it provides:
//! * configuring a new device
//! * starting a network
//! * joining, rejoining and leaving a network
//! * addressing
//! * neighbor discovery
//! * route discovery
//! * reception control
//! * routing

use core::slice;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use byte::BytesExt;
use byte::TryRead;
use management::NlmeEdScanConfirm;
use management::NlmeEdScanRequest;
use management::NlmeJoinConfirm;
use management::NlmeJoinRequest;
use management::NlmeJoinStatus;
use management::NlmeLeaveConfirm;
use management::NlmeLeaveIndication;
use management::NlmeLeaveRequest;
use management::NlmeLeaveStatus;
use management::NlmeNetworkDiscoveryConfirm;
use management::NlmeNetworkFormationConfirm;
use management::NlmeNetworkFormationRequest;
use management::NlmePermitJoiningConfirm;
use management::NlmePermitJoiningRequest;
use management::NlmeStartRouterConfirm;
use management::NlmeStartRouterRequest;
use management::RejoinNetwork;
use thiserror::Error;
use zigbee_mac::Address;
use zigbee_mac::AssociationStatus;
use zigbee_mac::MacShortAddress;
use zigbee_mac::PanId;
use zigbee_mac::mlme::MacError;
use zigbee_mac::mlme::Mlme;
use zigbee_mac::mlme::ScanType;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::StorageVec;
use zigbee_types::sync::Signal;
use zigbee_types::sync::with_timeout;

use crate::nwk::frame::CommandFrame as NwkCommandFrame;
use crate::nwk::frame::DataFrame as NwkDataFrame;
use crate::nwk::frame::Frame as NwkFrame;
use crate::nwk::frame::command::Command;
use crate::nwk::frame::command::end_device_timeout_request;
use crate::nwk::frame::command::end_device_timeout_request::EndDeviceTimeoutRequest;
use crate::nwk::frame::command::end_device_timeout_response::EndDeviceTimeoutResponse;
use crate::nwk::frame::command::leave::CommandOptions as LeaveCommandOptions;
use crate::nwk::frame::command::leave::Leave;
use crate::nwk::frame::command::rejoin_request;
use crate::nwk::frame::command::rejoin_request::RejoinRequest;
use crate::nwk::frame::command::rejoin_response::RejoinResponse;
use crate::nwk::frame::frame_control::DiscoverRoute;
use crate::nwk::frame::frame_control::FrameControl as NwkFrameControl;
use crate::nwk::frame::frame_control::FrameType as NwkFrameType;
use crate::nwk::frame::header::Header as NwkHeader;
use crate::nwk::nib;
use crate::nwk::nib::CapabilityInformation;
use crate::nwk::nib::DeviceType;
use crate::nwk::nib::MAX_PARENT_LINK_COST;
use crate::nwk::nib::NWK_BROADCAST_ADDRESS_MIN;
use crate::nwk::nib::NWK_BROADCAST_ALL;
use crate::nwk::nib::NWK_COORDINATOR_ADDRESS;
use crate::nwk::nib::NWK_UNASSIGNED_ADDRESS;
use crate::nwk::nib::Nib;
use crate::nwk::nib::NwkNeighbor;
use crate::nwk::nib::link_cost_from_lqi;
use crate::nwk::nib::relationship;
use crate::security::SecurityContext;

/// Network management entity
pub mod management;

// sentinel for `pending_timeout_request`: no request outstanding
const NO_PENDING_TIMEOUT: u8 = 0xff;

// poll attempts while waiting for the parent to deliver a buffered Rejoin
// Response (§3.4.7.1: indirect transmission for a sleepy child)
const REJOIN_RESPONSE_POLL_RETRIES: u8 = 20;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("mac error: {0}")]
    MacError(#[from] MacError),
    #[error("not joined to a network")]
    NotJoined,
    #[error("no transport key received from coordinator")]
    NoTransportKey,
    #[error("frame parse error")]
    ParseError,
    #[error("invalid frame")]
    InvalidFrame,
    #[error("security error: {0}")]
    SecurityError(#[from] crate::security::SecurityError),
}

impl From<byte::Error> for NetworkError {
    fn from(_: byte::Error) -> Self {
        Self::ParseError
    }
}

/// Network Layer Management Entity (§3.2.2).
///
/// Provides the management service access point (NLME-SAP) that allows
/// the next higher layer to interact with the NWK layer: network
/// discovery, formation, joining, rejoining, data transmission, etc.
pub struct Nlme<M> {
    mac: M,
    nwk_seq: AtomicU8,
    // requested timeout enum awaiting a response (§3.6.10.2); 0xff = none
    pending_timeout_request: AtomicU8,
    // Rejoin Response (§3.4.7) handed from the receive path to the rejoin
    // procedure waiting for it
    rejoin_response: Signal<RejoinResponse>,
    // NLME-LEAVE.indication (§3.2.2.19) raised by the receive path for the
    // higher layer, which decides whether to re-commission or rejoin
    leave_indication: Signal<NlmeLeaveIndication>,
}

impl<M> Nlme<M>
where
    M: Mlme,
{
    /// Creates a new instance owning the given MAC.
    ///
    /// Mirrors the MAC's hardware-provisioned IEEE address into
    /// `nwkIeeeAddress`, so it is valid even when resuming on a network
    /// without re-associating. The information bases must be initialized
    /// first.
    pub fn new(mac: M) -> Self {
        nib::get_ref().update_ieee_address(|value| *value = mac.ieee_address());
        Self {
            mac,
            nwk_seq: AtomicU8::new(0),
            pending_timeout_request: AtomicU8::new(NO_PENDING_TIMEOUT),
            rejoin_response: Signal::new(),
            leave_indication: Signal::new(),
        }
    }

    /// 3.2.2.19 - take a pending NLME-LEAVE.indication, if any.
    ///
    /// Reports that this device was removed from the network (device address
    /// `None`), or that a neighbor left it. The indication stays pending until
    /// taken.
    pub fn take_leave_indication(&self) -> Option<NlmeLeaveIndication> {
        self.leave_indication.try_take()
    }

    /// 3.2.2.19 - wait for the next NLME-LEAVE.indication.
    pub async fn wait_leave_indication(&self) -> NlmeLeaveIndication {
        self.leave_indication.wait().await
    }

    fn next_nwk_seq(&self) -> u8 {
        self.nwk_seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }

    /// Build a NWK data frame into `buf`, returning the total frame length.
    ///
    /// When `secure` is true the frame is encrypted with the active
    /// network key.
    fn build_nwk_data_frame(
        &self,
        destination: ShortAddress,
        secure: bool,
        payload: &[u8],
        buf: &mut [u8],
    ) -> Result<usize, NetworkError> {
        let nib = self.nib();
        let frame_control = NwkFrameControl(0)
            .set_frame_type(NwkFrameType::Data)
            .set_protocol_version(2)
            .set_discover_route(DiscoverRoute::Suppress)
            .set_security_flag(secure);

        let seq = self.next_nwk_seq();
        let header = NwkHeader {
            frame_control,
            destination,
            source: ShortAddress(*nib.network_address()),
            radius: 30,
            sequence_number: seq,
            destination_ieee: None,
            source_ieee: None,
            multicast_control: None,
            source_route_subframe: None,
        };

        if secure {
            let nwk_frame = NwkFrame::Data(NwkDataFrame { header, payload });
            let cx = SecurityContext::get();
            let len = cx.encrypt_nwk_frame_in_place(nwk_frame, buf)?;
            Ok(len)
        } else {
            let offset = &mut 0;
            buf.write_with(offset, header, ())?;
            let hdr_len = *offset;
            let payload_len = payload.len().min(buf.len() - hdr_len);
            buf[hdr_len..hdr_len + payload_len].copy_from_slice(&payload[..payload_len]);
            Ok(hdr_len + payload_len)
        }
    }

    /// Build a NWK command frame into `buf`, returning the total frame length
    /// (§3.3.2).
    ///
    /// When `secure` is false the frame is sent in the clear, as required for
    /// a Trust Center rejoin (§4.6.3.3.2).
    fn build_nwk_command_frame(
        &self,
        destination: ShortAddress,
        command: Command<'_>,
        secure: bool,
        buf: &mut [u8],
    ) -> Result<usize, NetworkError> {
        let nib = self.nib();
        // the destination IEEE address is carried whenever it is known
        // (§3.4.6.2, §3.4.11.2)
        let destination_ieee = self.neighbor_ieee_address(destination);
        let frame_control = NwkFrameControl(0)
            .set_frame_type(NwkFrameType::NwkCommand)
            .set_protocol_version(2)
            .set_discover_route(DiscoverRoute::Suppress)
            .set_security_flag(secure)
            .set_destination_ieee_flag(destination_ieee.is_some())
            .set_source_ieee_flag(true);

        let header = NwkHeader {
            frame_control,
            destination,
            source: ShortAddress(*nib.network_address()),
            radius: 1,
            sequence_number: self.next_nwk_seq(),
            destination_ieee,
            source_ieee: Some(*nib.ieee_address()),
            multicast_control: None,
            source_route_subframe: None,
        };

        if !secure {
            let offset = &mut 0;
            buf.write_with(offset, header, ())?;
            buf.write_with(offset, command, ())?;
            return Ok(*offset);
        }

        let nwk_frame = NwkFrame::NwkCommand(NwkCommandFrame { header, command });
        let cx = SecurityContext::get();
        let len = cx.encrypt_nwk_frame_in_place(nwk_frame, buf)?;
        Ok(len)
    }

    /// Build a failed NLME-JOIN.confirm (§3.2.2.13.3).
    fn join_failure(status: NlmeJoinStatus) -> NlmeJoinConfirm {
        NlmeJoinConfirm {
            status,
            network_address: ShortAddress(0xffff),
            extended_pan_id: IeeeAddress(0u64),
            channel: 0,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        }
    }

    /// Send an End Device Timeout Request to the parent (§3.6.10.2).
    ///
    /// Requests the timeout enumeration from `nwkEndDeviceTimeoutDefault`.
    /// The parent's response is consumed by the receive path and updates
    /// `nwkParentInformation` and the negotiated timeout in the NIB; no
    /// response means a legacy parent and leaves the NIB untouched.
    pub async fn send_end_device_timeout_request(&self) -> Result<(), NetworkError> {
        let requested_timeout = *self.nib().end_device_timeout_default();
        let command = Command::EndDeviceTimeoutRequest(EndDeviceTimeoutRequest {
            requested_timeout,
            // all bits reserved, must be 0 (§3.4.11.3.2)
            end_device_configuration: 0x00,
        });

        let mut buf = [0u8; 256];
        let len =
            self.build_nwk_command_frame(self.parent_short_address()?, command, true, &mut buf)?;
        self.pending_timeout_request
            .store(requested_timeout, Ordering::Relaxed);
        self.mac
            .transmit_data(self.parent_address()?, &buf[..len])
            .await?;
        Ok(())
    }

    /// Recommended maximum poll interval in milliseconds, allowing three
    /// keepalives per negotiated timeout period (§3.6.10.3).
    ///
    /// Returns `None` when no timeout was negotiated or the parent does not
    /// support the MAC data poll keepalive method.
    pub fn negotiated_poll_interval_ms(&self) -> Option<u32> {
        let nib = self.nib();
        let seconds = end_device_timeout_request::timeout_seconds(*nib.end_device_timeout())?;
        let parent_information = *nib.parent_information();
        if parent_information & EndDeviceTimeoutResponse::MAC_DATA_POLL_KEEPALIVE == 0 {
            return None;
        }
        Some(seconds * 1000 / 3)
    }

    /// Returns a reference to the global NIB singleton.
    pub fn nib(&self) -> &'static Nib {
        nib::get_ref()
    }

    /// Select parent candidates from the neighbor table (§3.6.1.4.1.1).
    fn select_parent_candidates(
        &self,
        extended_pan_id: IeeeAddress,
        join_as_router: bool,
    ) -> heapless::Vec<usize, 16> {
        let table = self.nib().neighbor_table();
        log::debug!("[NWK-JOIN] neighbor table: {table:#?}");
        let stack_profile = self.nib().stack_profile();

        // Determine the most recent update_id among all matching neighbors.
        //
        // §3.6.1.4.1.1: "the determination of most recent needs to take into
        // account that the update id will wrap back to zero."  We treat the
        // update_id as a modular counter: an id `b` is considered newer than
        // `a` when the signed difference `(b - a) as i8` is positive.
        let mut matching = table.iter().filter(|n| {
            n.extended_pan_id == extended_pan_id && n.permit_joining && n.potential_parent == 1
        });

        let Some(first) = matching.next() else {
            return heapless::Vec::new();
        };

        let best_update_id = matching
            .map(|n| n.update_id)
            .fold(first.update_id, |best, id| {
                // positive signed difference means id is newer
                if id.wrapping_sub(best).cast_signed() > 0 {
                    id
                } else {
                    best
                }
            });

        // Collect indices of eligible parents.
        let mut candidates: heapless::Vec<usize, 16> = table
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                // (1) Correct network
                n.extended_pan_id == extended_pan_id
                // (2) Accepting joins of correct type
                    && n.permit_joining
                    && if join_as_router {
                        n.router_capacity
                    } else {
                        n.end_device_capacity
                    }
                // (3) Link cost ≤ 3
                    && link_cost_from_lqi(n.lqi) <= MAX_PARENT_LINK_COST
                // (4) Potential parent
                    && n.potential_parent == 1
                // (5) Most recent update id
                    && n.update_id == best_update_id
            })
            .map(|(i, _)| i)
            .collect();

        // When nwkStackProfile == 1 prefer minimum depth (§3.6.1.4.1.1).
        if *stack_profile == 1 {
            candidates.sort_unstable_by_key(|&i| table[i].depth);
        }

        candidates
    }

    /// Build the IEEE 802.15.4 MAC `CapabilityInformation` from the NWK
    /// layer `CapabilityInformation` bitmap (Table 3-62).
    fn build_mac_capabilities(cap: &CapabilityInformation) -> zigbee_mac::CapabilityInformation {
        zigbee_mac::CapabilityInformation {
            // Bit 1 — Device type: 1 if joining as router (FFD)
            full_function_device: cap.device_type(),
            // Bit 2 — Power source
            mains_power: cap.power_source(),
            // Bit 3 — Receiver on when idle
            idle_receive: cap.receiver_on_when_idle(),
            // Bit 6 — Security capability
            frame_protection: cap.security_capability(),
            // Bit 7 — Allocate address
            allocate_address: cap.allocate_address(),
        }
    }

    /// Clear parent information and the negotiated end-device timeout; a
    /// fresh join invalidates both (§3.6.1.4.1.1, §3.6.10.2).
    fn reset_parent_negotiation(&self) {
        let nib = self.nib();
        nib.update_parent_information(|value| *value = 0);
        nib.update_end_device_timeout(|value| *value = NO_PENDING_TIMEOUT);
    }

    /// Find the parent's MAC address from the neighbor table.
    fn parent_address(&self) -> Result<Address, NetworkError> {
        let addr = Address::Short(
            PanId(*self.nib().panid()),
            MacShortAddress(self.parent_short_address()?.0),
        );
        Ok(addr)
    }

    /// The IEEE address of a neighbor, once it has been learned from a
    /// received frame carrying the source IEEE address field.
    fn neighbor_ieee_address(&self, network_address: ShortAddress) -> Option<IeeeAddress> {
        self.nib()
            .neighbor_table()
            .iter()
            .find(|n| n.network_address == network_address)
            .map(|n| n.extended_address)
            .filter(|ieee| ieee.0 != 0)
    }

    /// Record the IEEE address a received frame reported for its source
    /// (§3.4.6.2: it lets us address later commands by both addresses).
    fn learn_neighbor_ieee_address(&self, header: &NwkHeader<'_>) {
        let Some(source_ieee) = header.source_ieee else {
            return;
        };
        self.nib().update_neighbor_table(|table| {
            if let Some(neighbor) = table
                .iter_mut()
                .find(|n| n.network_address == header.source)
            {
                neighbor.extended_address = source_ieee;
            }
        });
    }

    /// Find the parent's network address from the neighbor table.
    fn parent_short_address(&self) -> Result<ShortAddress, NetworkError> {
        let table = self.nib().neighbor_table();
        let parent = table
            .iter()
            .find(|n| n.relationship == relationship::PARENT)
            .ok_or(NetworkError::NotJoined)?;
        Ok(parent.network_address)
    }

    /// Issue one MLME-POLL to the parent (§3.6.6).
    ///
    /// Returns the raw MAC payload length, or `None` when nothing was buffered.
    async fn poll_parent(&self, buf: &mut [u8]) -> Result<Option<usize>, NetworkError> {
        let coord_addr = self.parent_address()?;
        match self.mac.poll_data(coord_addr, buf).await {
            Ok((len, _lqi)) => Ok(Some(len)),
            Err(MacError::NoData) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Poll the parent once (MLME-POLL, §3.6.6) and process the retrieved NWK
    /// frame, if any.
    ///
    /// A sleepy end device only receives buffered unicasts by polling.
    /// Returns `Ok(None)` when nothing was buffered or the frame carried no
    /// application data.
    pub async fn poll_nwk_frame<'a>(
        &self,
        buf: &'a mut [u8],
    ) -> Result<Option<NwkDataFrame<'a>>, NetworkError> {
        let Some(len) = self.poll_parent(buf).await? else {
            return Ok(None);
        };
        self.process_received_nwk_frame(&mut buf[..len]).await
    }

    /// Passively wait for the next inbound NWK frame and process it.
    ///
    /// For devices with `rxOnWhenIdle = TRUE`, which receive directly instead
    /// of polling the parent for buffered unicasts. Returns `Ok(None)` when
    /// the frame carried no application data.
    pub async fn receive_nwk_frame<'a>(
        &self,
        buf: &'a mut [u8],
    ) -> Result<Option<NwkDataFrame<'a>>, NetworkError> {
        let (len, _lqi) = self.mac.receive(buf).await?;
        self.process_received_nwk_frame(&mut buf[..len]).await
    }

    /// Decrypt one inbound NWK frame and hand NWK commands to the NWK layer.
    async fn process_received_nwk_frame<'a>(
        &self,
        frame_buf: &'a mut [u8],
    ) -> Result<Option<NwkDataFrame<'a>>, NetworkError> {
        let cx = SecurityContext::get();
        let nwk_frame = cx.decrypt_nwk_frame_in_place(frame_buf)?;

        match nwk_frame {
            NwkFrame::Data(data_frame) => {
                self.learn_neighbor_ieee_address(&data_frame.header);
                Ok(Some(data_frame))
            }
            // NWK command frames (link status, route requests, rejoin, ...) ride
            // the same receive path; let the NWK layer process them, then report
            // "no data" so the APS/ZDO caller skips this frame.
            NwkFrame::NwkCommand(command_frame) => {
                self.learn_neighbor_ieee_address(&command_frame.header);
                self.handle_nwk_command(&command_frame.header, command_frame.command)
                    .await;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Process an inbound NWK command frame (§3.4).
    ///
    /// Scaffolding extension point: every NWK command variant is matched so a
    /// handler can be filled in. Most are not yet acted upon — they are logged
    /// and ignored. Returning unit keeps the caller's receive loop simple; add
    /// a result type here if a handler needs to surface failures.
    async fn handle_nwk_command(&self, header: &NwkHeader<'_>, command: Command<'_>) {
        match command {
            // routing (§3.4.1–3.4.2): a sleepy end device routes via its parent,
            // so route discovery is currently a no-op.
            Command::RouteRequest(_) => log::trace!("[NWK] route request (ignored)"),
            Command::RouteReply(_) => log::trace!("[NWK] route reply (ignored)"),
            Command::RouteRecord(_) => log::trace!("[NWK] route record (ignored)"),
            // network maintenance (§3.4.3, §3.4.9, §3.4.10).
            Command::NetworkStatus(_) => log::trace!("[NWK] network status (ignored)"),
            Command::NetworkReport(_) => log::trace!("[NWK] network report (ignored)"),
            Command::NetworkUpdate(_) => log::trace!("[NWK] network update (ignored)"),
            Command::Leave(leave) => self.handle_leave_command(header, &leave).await,
            // rejoin (§3.4.6–3.4.7): a request is parent-side only; a response
            // is handed to the rejoin procedure waiting for it, which may be
            // driven by another task than this receive path
            Command::RejoinRequest(_) => log::trace!("[NWK] rejoin request (ignored)"),
            Command::RejoinResponse(response) => {
                log::debug!(
                    "[NWK] rejoin response: address={:?}, status={:#04x}",
                    response.network_address,
                    response.status
                );
                if response.status == u8::from(AssociationStatus::Successful) {
                    // §3.4.7.3.1: the parent may hand out a different address
                    // than the one the device rejoined with
                    self.nib()
                        .update_network_address(|value| *value = response.network_address.0);
                }
                self.rejoin_response.signal(response);
            }
            // link management (§3.4.8, §3.4.13).
            Command::LinkStatus(_) => log::trace!("[NWK] link status (ignored)"),
            Command::LinkPowerDelta(_) => log::trace!("[NWK] link power delta (ignored)"),
            // end-device timeout requests are parent-side only (§3.4.11)
            Command::EndDeviceTimeoutRequest(_) => {
                log::trace!("[NWK] end-device timeout request (ignored)");
            }
            // §3.6.10.2: on SUCCESS store the Parent Information bitmask and
            // the negotiated timeout; otherwise the parent keeps its default.
            // TODO: timeout-request keepalive method (bit 1) and Parent Link
            // Failure (NWK status 0x09) on keepalive failure
            Command::EndDeviceTimeoutResponse(response) => {
                let pending = self
                    .pending_timeout_request
                    .swap(NO_PENDING_TIMEOUT, Ordering::Relaxed);
                if response.status == EndDeviceTimeoutResponse::STATUS_SUCCESS
                    && pending != NO_PENDING_TIMEOUT
                {
                    let nib = self.nib();
                    nib.update_parent_information(|value| *value = response.parent_information);
                    nib.update_end_device_timeout(|value| *value = pending);
                    log::info!(
                        "[NWK] end-device timeout negotiated: enum={pending}, parent_information={:#04x}",
                        response.parent_information
                    );
                } else {
                    log::warn!(
                        "[NWK] end-device timeout response rejected or unexpected (status={:#04x})",
                        response.status
                    );
                }
            }
            Command::Reserved(id) => log::trace!("[NWK] reserved command {id:#04x} (ignored)"),
        }
    }

    /// Process an inbound Leave command frame (§3.6.1.10.3).
    ///
    /// TODO: a router receiving a notification from its parent with the remove
    /// children sub-field set must rebroadcast the leave to its own children.
    async fn handle_leave_command(&self, header: &NwkHeader<'_>, leave: &Leave) {
        let options = leave.command_options;
        let is_from_parent = self
            .parent_short_address()
            .is_ok_and(|parent| parent == header.source);

        if !options.request() {
            // notification: the sender left the network on its own initiative —
            // it is no longer a neighbor regardless of the rejoin flag.
            self.nib().update_neighbor_table(|table| {
                table.retain(|n| n.network_address != header.source);
            });

            if is_from_parent {
                // our parent has dropped us: we are no longer on the network,
                // and the indication carries a NULL device address (§3.6.1.10.3)
                self.nib().update_extended_panid(|value| *value = 0);
                log::info!("[NWK] removed by parent (leave notification)");
                self.leave_indication.signal(NlmeLeaveIndication {
                    device_address: None,
                    rejoin: options.rejoin(),
                });
            } else {
                log::debug!(
                    "[NWK] neighbor {:?} left the network (rejoin={})",
                    header.source_ieee,
                    options.rejoin()
                );
                self.leave_indication.signal(NlmeLeaveIndication {
                    device_address: header.source_ieee,
                    rejoin: options.rejoin(),
                });
            }
            return;
        }

        // request == 1: someone is asking us to leave (§3.6.1.10.3.1). A leave
        // request is sent by the parent itself, so its NWK source address is
        // the sender the spec tests.
        let from_parent = self
            .parent_short_address()
            .is_ok_and(|parent| parent == header.source);
        if !self.accepts_leave_request(header.destination, from_parent) {
            return;
        }

        if let Err(e) = self
            .leave_network(options.remove_children(), options.rejoin())
            .await
        {
            log::warn!("[NWK] failed to announce leave: {e:?}");
        }
    }

    /// Validate a leave request against §3.6.1.10.3.1, which governs both the
    /// NWK Leave (request) command and the ZDO Mgmt_Leave_req.
    ///
    /// `destination` is the address the request was addressed to.
    /// `from_parent` says whether it reached this device over the parent link:
    /// the spec tests the MAC source address of the delivering frame, which is
    /// the parent for anything a child receives — not the originator of a
    /// relayed ZDP request.
    pub fn accepts_leave_request(&self, destination: ShortAddress, from_parent: bool) -> bool {
        // step 1: the coordinator never leaves, and a broadcast request is
        // dropped without further processing
        if *self.nib().network_address() == NWK_COORDINATOR_ADDRESS
            || destination.0 >= NWK_BROADCAST_ADDRESS_MIN
        {
            log::trace!("[NWK] leave request dropped (coordinator or broadcast destination)");
            return false;
        }

        // step 2: a router honors any sender while nwkLeaveRequestAllowed is
        // set, ignoring the neighbor relationship
        if self.nib().capability_information().device_type() {
            let allowed = *self.nib().leave_request_allowed();
            if !allowed {
                log::trace!("[NWK] leave request refused (nwkLeaveRequestAllowed is FALSE)");
            }
            return allowed;
        }

        // step 3: an end device is only removed by its parent
        if !from_parent {
            log::trace!("[NWK] leave request refused (sender is not our parent)");
        }
        from_parent
    }

    /// 3.2.2.18
    ///
    /// Only self-removal (`device_address` `None` or this device's own IEEE
    /// address) is implemented: removing a child requires acting as its
    /// parent, which this stack does not yet support.
    pub async fn leave(&self, request: NlmeLeaveRequest) -> NlmeLeaveConfirm {
        if *self.nib().network_address() == NWK_UNASSIGNED_ADDRESS {
            return NlmeLeaveConfirm {
                status: NlmeLeaveStatus::InvalidRequest,
                device_address: request.device_address,
            };
        }

        let is_self = request
            .device_address
            .is_none_or(|addr| addr == *self.nib().ieee_address());
        if !is_self {
            return NlmeLeaveConfirm {
                status: NlmeLeaveStatus::UnknownDevice,
                device_address: request.device_address,
            };
        }

        let status = match self
            .leave_network(request.remove_children, request.rejoin)
            .await
        {
            Ok(()) => NlmeLeaveStatus::Success,
            Err(_) => NlmeLeaveStatus::MacError,
        };
        NlmeLeaveConfirm {
            status,
            device_address: None,
        }
    }

    /// §3.6.1.10.1 + §3.6.1.10.4: announce this device's own removal from the
    /// network, then apply the local leave procedure.
    ///
    /// The local leave runs whether or not the announcement made it onto the
    /// air; the transmit result only feeds the NLME-LEAVE.confirm status.
    async fn leave_network(&self, remove_children: bool, rejoin: bool) -> Result<(), NetworkError> {
        let result = self.announce_leave(remove_children, rejoin).await;
        self.local_leave(rejoin);
        result
    }

    /// §3.6.1.10.1: transmit this device's own leave command frame, with the
    /// request sub-field set to 0 — a notification, never a request addressed
    /// to someone else.
    async fn announce_leave(
        &self,
        remove_children: bool,
        rejoin: bool,
    ) -> Result<(), NetworkError> {
        let is_router_or_coordinator = self.nib().capability_information().device_type()
            || *self.nib().network_address() == NWK_COORDINATOR_ADDRESS;

        let mut buf = [0u8; 64];
        if is_router_or_coordinator {
            let command = Command::Leave(Leave {
                command_options: LeaveCommandOptions(0)
                    .set_remove_children(remove_children)
                    .set_rejoin(rejoin),
            });
            let len = self.build_nwk_command_frame(
                ShortAddress(NWK_BROADCAST_ALL),
                command,
                true,
                &mut buf,
            )?;
            self.mac
                .transmit_data(
                    Address::Short(
                        PanId(*self.nib().panid()),
                        MacShortAddress(NWK_BROADCAST_ALL),
                    ),
                    &buf[..len],
                )
                .await?;
            return Ok(());
        }

        // an end device unicasts to its parent and sends the remove children
        // sub-field as 0; only the rejoin flag is carried over
        let command = Command::Leave(Leave {
            command_options: LeaveCommandOptions(0).set_rejoin(rejoin),
        });
        let len =
            self.build_nwk_command_frame(self.parent_short_address()?, command, true, &mut buf)?;
        let result = self
            .mac
            .transmit_data(self.parent_address()?, &buf[..len])
            .await;
        // §3.6.1.10.1: an end device clears the extended PAN id right after
        // transmitting, ahead of the rest of the local leave process.
        self.nib().update_extended_panid(|value| *value = 0);
        result.map_err(Into::into)
    }

    /// §3.6.1.10.4: the local half of the leave procedure.
    fn local_leave(&self, rejoin: bool) {
        if rejoin {
            // step 1: with Rejoin set the NIB is kept, so the remembered
            // network is still there for a later NLME-JOIN.request with
            // `RejoinNetwork::NwkRejoin`; driving that is the higher layer's
            // call, which the NLME-LEAVE.indication/confirm hands it.
            log::info!("[NWK] left the network, rejoin requested");
            return;
        }

        self.clear_nib_on_leave();
    }

    /// §3.6.1.10.4 (Rejoin = FALSE): clear the NIB attributes describing
    /// network membership, leaving the device unjoined.
    fn clear_nib_on_leave(&self) {
        let nib = self.nib();
        nib.update_neighbor_table(|value| value.clear());
        nib.update_route_table(|value| value.clear());
        nib.update_manager_addr(|value| *value = 0x0000);
        nib.update_update_id(|value| *value = 0x00);
        nib.update_network_address(|value| *value = NWK_UNASSIGNED_ADDRESS);
        nib.update_group_idtable(|value| value.clear());
        nib.update_extended_panid(|value| *value = 0);
        nib.update_route_record_table(|value| value.clear());
        nib.update_is_concentrator(|value| *value = false);
        nib.update_concentrator_radius(|value| *value = 0);
        nib.update_security_material_set(|value| value.clear());
        nib.update_active_key_seq_number(|value| *value = 0x00);
        nib.update_address_map(|value| value.clear());
        nib.update_panid(|value| *value = 0xffff);
        nib.update_tx_total(|value| *value = 0);
        nib.update_parent_information(|value| *value = 0x00);
    }

    /// 3.2.2.3
    pub async fn network_discovery(
        &self,
        channels: core::ops::Range<u8>,
        duration: u8,
    ) -> Result<NlmeNetworkDiscoveryConfirm, NetworkError> {
        let scan_result = self
            .mac
            .scan_network(ScanType::Active, channels, duration)
            .await?;

        // Populate the neighbor table with mandatory fields (Table 3-63)
        // and optional discovery-time fields (Table 3-64).
        let neighbor_table = scan_result
            .pan_descriptor
            .iter()
            .filter_map(|pd| match pd.coord_address {
                Address::Short(_pan_id, short_address) => Some(NwkNeighbor {
                    // a beacon carries no IEEE address; learned later from
                    // received frames
                    extended_address: IeeeAddress(0),
                    network_address: ShortAddress(short_address.0),
                    device_type: if short_address.0 == NWK_COORDINATOR_ADDRESS {
                        DeviceType::Coordinator
                    } else {
                        DeviceType::Router
                    },
                    rx_on_when_idle: false,
                    end_device_configuration: 0,
                    relationship: relationship::NONE,
                    transmit_failure: 0,
                    lqi: pd.link_quality,
                    outgoing_cost: 0,
                    age: 0,
                    keepalive_received: false,
                    // Table 3-64: optional discovery-time fields
                    extended_pan_id: pd.zigbee_beacon.extended_pan_id,
                    logical_channel: pd.channel,
                    depth: pd.zigbee_beacon.stack_profile.device_depth(),
                    permit_joining: pd.superframe_spec.association_permit,
                    // a device is a potential parent if it can accept a child of
                    // either type; select_parent_candidates applies the
                    // join-type-specific capacity check (Table 3-64)
                    potential_parent: u8::from(
                        pd.zigbee_beacon.stack_profile.router_capacity()
                            || pd.zigbee_beacon.stack_profile.end_device_capacity()
                            || short_address.0 == NWK_COORDINATOR_ADDRESS,
                    ),
                    router_capacity: pd.zigbee_beacon.stack_profile.router_capacity(),
                    end_device_capacity: pd.zigbee_beacon.stack_profile.end_device_capacity(),
                    update_id: pd.zigbee_beacon.update_id,
                    pan_id: pd.coord_pan_id.0,
                }),
                Address::Extended(_, _) => None,
            })
            .collect();

        self.nib()
            .update_neighbor_table(|value| *value = StorageVec(neighbor_table));

        // Build network descriptors for the confirm primitive.
        let network_descriptors = scan_result
            .pan_descriptor
            .into_iter()
            .map(From::from)
            .collect();

        Ok(NlmeNetworkDiscoveryConfirm {
            network_descriptor: network_descriptors,
        })
    }

    /// 3.2.2.5
    #[allow(clippy::unused_async)]
    pub async fn network_formation(
        &self,
        _request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm {
        todo!()
    }

    /// 3.2.2.7
    // figure 3-39
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn permit_joining(
        &self,
        _request: NlmePermitJoiningRequest,
    ) -> NlmePermitJoiningConfirm {
        NlmePermitJoiningConfirm {
            status: NlmeJoinStatus::InvalidRequest,
        }
    }

    /// 3.2.2.9
    #[allow(clippy::unused_async)]
    pub async fn start_router(&self, _request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm {
        todo!()
    }

    /// 3.2.2.11
    #[allow(clippy::unused_async)]
    pub async fn ed_scan(&self, _request: NlmeEdScanRequest) -> NlmeEdScanConfirm {
        todo!()
    }

    /// 3.2.2.13
    // the association candidate loop is a single state machine; splitting it
    // up would scatter the §3.6.1.4.1.1 bookkeeping across functions
    #[allow(clippy::too_many_lines)]
    pub async fn join(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm {
        if request.rejoin_network == RejoinNetwork::NwkRejoin {
            return self.nwk_rejoin(request).await;
        }
        let fail = Self::join_failure;

        // --- Validate the request (§3.2.2.13.3) ---

        // Only MAC association is handled here.
        // Orphan (0x01) and channel change (0x03) are not yet implemented.
        if request.rejoin_network != RejoinNetwork::Association {
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // A device already joined must not re-associate (§3.6.1.4.1.1).
        if *self.nib().network_address() != 0xffff {
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // --- Parent selection (§3.6.1.4.1.1) ---

        // Whether joining as router or end device, set nwkParentInformation
        // to 0 before searching (spec requirement).
        self.reset_parent_negotiation();

        let join_as_router = request.capability_information.device_type();

        let candidates = self.select_parent_candidates(request.extended_pan_id, join_as_router);

        if candidates.is_empty() {
            log::warn!("[NWK-JOIN] no suitable neighbors");
            return fail(NlmeJoinStatus::NotPermitted);
        }

        log::debug!("[NWK-JOIN] neighbor candidates: {candidates:?}");

        // Build MAC CapabilityInformation from NWK CapabilityInformation
        // bitmap (Table 3-62).
        let mac_caps = Self::build_mac_capabilities(&request.capability_information);

        // Store in NIB (§3.6.1.4.1.1: "the capability information shall be
        // stored as the value of the nwkCapabilityInformation NIB attribute").
        self.nib()
            .update_capability_information(|value| *value = request.capability_information);

        // --- Try each candidate in order (§3.6.1.4.1.1) ---
        let mut last_status = NlmeJoinStatus::NotPermitted;

        for &candidate_idx in &candidates {
            // Read the neighbor info we need before the async call.
            let table = self.nib().neighbor_table();
            let neighbor = &table[candidate_idx];
            let channel = neighbor.logical_channel;
            let pan_id = PanId(neighbor.pan_id);
            let dest = Address::Short(pan_id, MacShortAddress(neighbor.network_address.0));
            drop(table);

            // Issue MLME-ASSOCIATE.request to MAC sub-layer.
            match self.mac.associate(channel, dest, mac_caps).await {
                Ok(response) => {
                    match response.status {
                        AssociationStatus::Successful => {
                            // --- Success: update NIB (§3.6.1.4.1.1) ---
                            let assigned_addr = response.association_address;
                            self.nib()
                                .update_network_address(|value| *value = assigned_addr.0);
                            self.nib()
                                .update_ieee_address(|value| *value = response.device_address);
                            self.nib()
                                .update_extended_panid(|value| *value = request.extended_pan_id.0);
                            self.nib().update_panid(|value| *value = pan_id.0);

                            // Read parent fields before the clearing loop
                            // zeroes them (§3.6.1.4.1.1).
                            let parent_update_id =
                                self.nib().neighbor_table()[candidate_idx].update_id;
                            let parent_channel =
                                self.nib().neighbor_table()[candidate_idx].logical_channel;
                            self.nib()
                                .update_update_id(|value| *value = parent_update_id);

                            // Update the neighbor table: set the relationship
                            // field to 0x00 (parent) and clear optional
                            // Table 3-64 fields on all entries (they should
                            // not be retained after joining).
                            // TODO: retain only entries belonging to the
                            // joined network.
                            self.nib().update_neighbor_table(|table| {
                                table[candidate_idx].relationship = relationship::PARENT;
                                for neighbor in table.iter_mut() {
                                    neighbor.extended_pan_id = IeeeAddress(0);
                                    neighbor.logical_channel = 0;
                                    neighbor.depth = 0;
                                    neighbor.permit_joining = false;
                                    neighbor.potential_parent = 0;
                                    neighbor.router_capacity = false;
                                    neighbor.end_device_capacity = false;
                                    neighbor.update_id = 0;
                                    neighbor.pan_id = 0xffff;
                                }
                            });

                            return NlmeJoinConfirm {
                                status: NlmeJoinStatus::Success,
                                network_address: assigned_addr,
                                extended_pan_id: request.extended_pan_id,
                                channel: parent_channel,
                                enhanced_beacon_type: false,
                                mac_interface_index: 0u8,
                            };
                        }
                        AssociationStatus::NetworkAtCapacity => {
                            // Mark this neighbor as not a potential parent so
                            // we don't retry (§3.6.1.4.1.1).
                            self.nib().update_neighbor_table(|table| {
                                table[candidate_idx].potential_parent = 0;
                            });
                            last_status = NlmeJoinStatus::PanAtCapacity;
                        }
                        AssociationStatus::AccessDenied => {
                            self.nib().update_neighbor_table(|table| {
                                table[candidate_idx].potential_parent = 0;
                            });
                            last_status = NlmeJoinStatus::PanAccessDenied;
                        }
                        _ => {
                            // Other status codes (FastAssociationSuccesful,
                            // HoppingSequenceOffsetDuplication, etc.) are
                            // treated as a generic MAC-level failure.
                            last_status = NlmeJoinStatus::MacError;
                        }
                    }
                }
                Err(_mac_err) => {
                    // MAC-level failure (no ack, radio error, etc.)
                    last_status = NlmeJoinStatus::MacError;
                }
            }
        }

        // All candidates exhausted — return the last error status.
        fail(last_status)
    }

    /// NLME-JOIN.request with `RejoinNetwork::NwkRejoin` (§3.2.2.13.3).
    ///
    /// Reconnects to a network this device remembers — extended PAN id,
    /// parent, network key — without a fresh MLME-SCAN: it unicasts a Rejoin
    /// Request (§3.4.6) to the previously known parent and waits for the
    /// Rejoin Response (§3.4.7).
    ///
    /// `request.security_enabled` selects the rejoin flavour: `true` secures
    /// the Rejoin Request with the active network key (Secure Rejoin,
    /// §4.6.3.3.1), `false` sends it unsecured (Trust Center Rejoin,
    /// §4.6.3.3.2) for a device that no longer holds the key — the caller is
    /// then responsible for obtaining the current network key from the Trust
    /// Center.
    async fn nwk_rejoin(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm {
        let fail = Self::join_failure;

        // a rejoin only makes sense against a network this device already
        // remembers (BDB §7.1 step 4 only reaches here when
        // bdbNodeIsOnANetwork is TRUE and the extended PAN id is known).
        if *self.nib().network_address() == 0xffff
            || *self.nib().extended_panid() != request.extended_pan_id.0
        {
            return fail(NlmeJoinStatus::InvalidRequest);
        }
        let Ok(parent) = self.parent_short_address() else {
            return fail(NlmeJoinStatus::InvalidRequest);
        };

        // whether joining or rejoining, nwkParentInformation is reset
        // (§3.2.2.13.3)
        self.reset_parent_negotiation();
        self.nib()
            .update_capability_information(|value| *value = request.capability_information);

        let command = Command::RejoinRequest(RejoinRequest {
            // the Capability Information bit layout (Table 3-62) is shared
            // with `nwk::nib::CapabilityInformation`
            capability_information: rejoin_request::CapabilityInformation(
                request.capability_information.0,
            ),
        });

        let mut buf = [0u8; 128];
        let Ok(len) =
            self.build_nwk_command_frame(parent, command, request.security_enabled, &mut buf)
        else {
            return fail(NlmeJoinStatus::MacError);
        };
        let Ok(parent_addr) = self.parent_address() else {
            return fail(NlmeJoinStatus::InvalidRequest);
        };

        // arm before transmitting: the response is delivered by the receive
        // path, which may be this procedure's own poll or a concurrent loop
        self.rejoin_response.reset();
        if self
            .mac
            .transmit_data(parent_addr, &buf[..len])
            .await
            .is_err()
        {
            return fail(NlmeJoinStatus::MacError);
        }

        let Some(response) = self
            .await_rejoin_response(REJOIN_RESPONSE_POLL_RETRIES)
            .await
        else {
            return fail(NlmeJoinStatus::MacError);
        };

        if response.status == u8::from(AssociationStatus::NetworkAtCapacity) {
            return fail(NlmeJoinStatus::PanAtCapacity);
        }
        if response.status == u8::from(AssociationStatus::AccessDenied) {
            return fail(NlmeJoinStatus::PanAccessDenied);
        }
        if response.status != u8::from(AssociationStatus::Successful) {
            return fail(NlmeJoinStatus::MacError);
        }

        // the receive path stored the assigned address; the hardware filter has
        // to follow it
        self.mac.set_short_address(response.network_address).await;

        NlmeJoinConfirm {
            status: NlmeJoinStatus::Success,
            network_address: response.network_address,
            extended_pan_id: request.extended_pan_id,
            // the operating channel isn't tracked in the NIB — the caller is
            // responsible for having the radio already tuned to it (BDB
            // §7.1 step 4 passes ScanChannels=0: no scan is performed here).
            channel: 0,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        }
    }

    /// Wait for the receive path to deliver the Rejoin Response (§3.4.7).
    ///
    /// A concurrently running receive loop may deliver it; otherwise the
    /// bounded polling below does.
    async fn await_rejoin_response(&self, retries: u8) -> Option<RejoinResponse> {
        with_timeout(
            self.rejoin_response.wait(),
            self.poll_until_rejoin_response(retries),
        )
        .await
        // the response may have arrived in the very poll that ended the polling
        .or_else(|| self.rejoin_response.try_take())
    }

    /// Poll the parent until the Rejoin Response has been processed or
    /// `retries` polls went unanswered.
    ///
    /// Drives reception for a caller whose receive loop is not running yet; a
    /// data frame cannot be dispatched from here and is left to that loop.
    async fn poll_until_rejoin_response(&self, retries: u8) {
        let mut buf = [0u8; 128];
        for _ in 0..retries {
            if self.rejoin_response.is_signaled() {
                return;
            }
            match self.poll_parent(&mut buf).await {
                Ok(Some(len)) => match self.process_received_nwk_frame(&mut buf[..len]).await {
                    Ok(Some(_)) => log::debug!("[NWK-REJOIN] polled data frame dropped"),
                    Ok(None) => (),
                    Err(e) => log::debug!("[NWK-REJOIN] polled frame not processed: {e:?}"),
                },
                Ok(None) => (),
                Err(e) => log::debug!("[NWK-REJOIN] poll failed: {e:?}"),
            }
        }
    }

    /// Poll the coordinator for pending data, strip the NWK header, and
    /// return the APS payload (§3.6.2).
    pub async fn poll_nwk_data<'a>(
        &self,
        buf: &'a mut [u8],
        retries: u8,
    ) -> Result<NwkDataFrame<'a>, NetworkError> {
        for _ in 0..retries {
            // SAFETY: `buf` is mutably borrowed once at a time
            // need to get rid of the 'a lifetime in the loop
            // &mut buf is still guaranteed within 'a
            let buf = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr(), buf.len()) };
            match self.poll_nwk_frame(buf).await {
                Ok(Some(data_frame)) => {
                    return Ok(data_frame);
                }
                // keep polling: nothing buffered, a NWK command frame, or ambient
                // traffic the pre-key joiner cannot decode (SecurityError/ParseError).
                // the NWK-unsecured transport-key (§4.6.3.7.2) stays buffered for a
                // later poll
                Ok(None) | Err(NetworkError::SecurityError(_) | NetworkError::ParseError) => (),
                Err(e) => return Err(e),
            }
        }

        Err(NetworkError::MacError(MacError::NoData))
    }

    /// Broadcast an NWK data frame (§3.6.5).
    ///
    /// Wraps `payload` in a NWK header addressed to `destination` and
    /// transmits it as a MAC broadcast.
    ///
    /// When `secure` is true the NWK frame is encrypted with the
    /// active network key.
    pub async fn broadcast_data(
        &self,
        destination: ShortAddress,
        secure: bool,
        payload: &[u8],
    ) -> Result<(), NetworkError> {
        let mut buf = [0u8; 256];
        let total_len = self.build_nwk_data_frame(destination, secure, payload, &mut buf)?;
        // §3.6.5: a sleepy end device unicasts broadcasts to its parent,
        // which relays them into the network on its behalf
        let mac_dest = self.parent_address()?;
        self.mac.transmit_data(mac_dest, &buf[..total_len]).await?;
        Ok(())
    }

    /// Send an NWK data frame to a specific destination (§3.6.3).
    ///
    /// Wraps `payload` in a NWK header addressed to `destination` and
    /// transmits it via the parent (for end devices) or directly.
    ///
    /// When `secure` is true the NWK frame is encrypted with the
    /// active network key.
    pub async fn send_data(
        &self,
        destination: ShortAddress,
        secure: bool,
        payload: &[u8],
    ) -> Result<(), NetworkError> {
        let mut buf = [0u8; 256];
        let total_len = self.build_nwk_data_frame(destination, secure, payload, &mut buf)?;
        // end devices route via parent
        let mac_dest = self.parent_address()?;
        self.mac.transmit_data(mac_dest, &buf[..total_len]).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::future::Future;

    use zigbee_mac::AssociationStatus;
    use zigbee_mac::mlme::AssociationResponse;
    use zigbee_mac::mlme::MacError;
    use zigbee_mac::mlme::ScanResult;
    use zigbee_mac::mlme::ScanType;

    use super::*;

    // tests share a global NIB singleton — serialize access
    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // -------------------------------------------------------------------
    // Minimal async block_on — the mock futures resolve immediately so a
    // single poll is sufficient.
    // -------------------------------------------------------------------

    #[allow(clippy::panic)]
    fn block_on<F: Future>(f: F) -> F::Output {
        use core::pin::pin;
        use core::task::Context;
        use core::task::Poll;
        use core::task::RawWaker;
        use core::task::RawWakerVTable;
        use core::task::Waker;

        fn noop(_: *const ()) {}
        fn clone(p: *const ()) -> RawWaker {
            RawWaker::new(p, &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);

        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut f = pin!(f);

        match f.as_mut().poll(&mut cx) {
            Poll::Ready(val) => val,
            Poll::Pending => panic!("block_on: future returned Pending"),
        }
    }

    mockall::mock! {
        Mlme {}
        impl Mlme for Mlme {
            fn ieee_address(&self) -> IeeeAddress;
            async fn set_short_address(&self, address: ShortAddress);
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

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    /// Create a default `NwkNeighbor` pre-filled for parent selection.
    fn make_neighbor(pan_id: u16, short_addr: u16, epid: u64, lqi: u8, depth: u8) -> NwkNeighbor {
        NwkNeighbor {
            extended_address: IeeeAddress(0),
            network_address: ShortAddress(short_addr),
            device_type: if short_addr == 0 {
                DeviceType::Coordinator
            } else {
                DeviceType::Router
            },
            rx_on_when_idle: false,
            end_device_configuration: 0,
            relationship: 0x03,
            transmit_failure: 0,
            lqi,
            outgoing_cost: 0,
            age: 0,
            keepalive_received: false,
            extended_pan_id: IeeeAddress(epid),
            logical_channel: 11,
            depth,
            permit_joining: true,
            potential_parent: 1,
            router_capacity: true,
            end_device_capacity: true,
            update_id: 0,
            pan_id,
        }
    }

    fn make_nlme(mut mac: MockMlme) -> (std::sync::MutexGuard<'static, ()>, Nlme<MockMlme>) {
        let guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        use crate::nwk::nib;
        nib::try_init();
        nib::reset();
        // the security context used by frame builders also needs the AIB
        crate::aps::aib::try_init();
        // reset() only rewrites fields with a declared default; StorageVec fields
        // (no default) persist across tests in the global NIB, so clear explicitly
        nib::get_ref().update_neighbor_table(|value| *value = StorageVec::new());
        mac.expect_ieee_address()
            .return_const(IeeeAddress(0xa4c1_0000_0000_0001));
        (guard, Nlme::new(mac))
    }

    fn default_join_request(epid: u64) -> NlmeJoinRequest {
        NlmeJoinRequest {
            extended_pan_id: IeeeAddress(epid),
            rejoin_network: RejoinNetwork::Association,
            capability_information: CapabilityInformation(0x80),
            security_enabled: false,
        }
    }

    // -------------------------------------------------------------------
    // select_parent_candidates tests
    // -------------------------------------------------------------------

    #[test]
    fn select_parent_no_neighbors() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_filters_by_extended_pan_id() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        // neighbor on the correct network
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0))
            .unwrap();
        // neighbor on a different network
        table
            .push(make_neighbor(0xBBBB, 0x0001, 0x9999, 200, 0))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], 0);
    }

    #[test]
    fn select_parent_filters_by_link_cost() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        // good LQI => low cost => eligible
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0))
            .unwrap();
        // bad LQI => high cost => filtered out
        table
            .push(make_neighbor(0xAAAA, 0x0001, 0x1234, 10, 0))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], 0);
    }

    #[test]
    fn select_parent_filters_by_end_device_capacity() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.end_device_capacity = false;
        table.push(n).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_filters_by_router_capacity() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.router_capacity = false;
        table.push(n).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), true);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_end_device_accepts_router_without_router_capacity() {
        // a router parent that accepts end devices but not routers
        // (router_capacity == false, end_device_capacity == true) must remain a
        // valid parent for an end-device join
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x1234, 0x1234, 200, 0);
        n.router_capacity = false;
        n.end_device_capacity = true;
        table.push(n).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn select_parent_sorts_by_depth_for_stack_profile_1() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        nlme.nib().update_stack_profile(|value| *value = 1);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 3))
            .unwrap();
        table
            .push(make_neighbor(0xAAAA, 0x0001, 0x1234, 200, 1))
            .unwrap();
        table
            .push(make_neighbor(0xAAAA, 0x0002, 0x1234, 200, 2))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert_eq!(candidates.len(), 3);
        // sorted: depth 1 (idx 1), depth 2 (idx 2), depth 3 (idx 0)
        assert_eq!(candidates[0], 1);
        assert_eq!(candidates[1], 2);
        assert_eq!(candidates[2], 0);
    }

    #[test]
    fn select_parent_filters_not_permitting_join() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.permit_joining = false;
        table.push(n).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_filters_non_potential_parent() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.potential_parent = 0;
        table.push(n).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_prefers_most_recent_update_id() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n1 = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n1.update_id = 5;
        table.push(n1).unwrap();
        let mut n2 = make_neighbor(0xAAAA, 0x0001, 0x1234, 200, 0);
        n2.update_id = 3;
        table.push(n2).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let candidates = nlme.select_parent_candidates(IeeeAddress(0x1234), false);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], 0);
    }

    // -------------------------------------------------------------------
    // join() integration tests (using MockMlme)
    // -------------------------------------------------------------------

    #[test]
    fn join_successful_association() {
        let mut mac = MockMlme::new();
        mac.expect_associate().returning(|_, _, _| {
            Ok(AssociationResponse {
                device_address: IeeeAddress(0),
                association_address: ShortAddress(0x1234),
                status: AssociationStatus::Successful,
            })
        });

        let (_guard, nlme) = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));

        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(confirm.network_address.0, 0x1234);
        assert_eq!(confirm.extended_pan_id.0, 0xDEAD);
        assert_eq!(confirm.channel, 11);

        assert_eq!(*nlme.nib().network_address(), 0x1234);
        assert_eq!(*nlme.nib().extended_panid(), 0xDEAD);
        assert_eq!(*nlme.nib().panid(), 0xAAAA);
        assert_eq!(*nlme.nib().update_id(), 0);

        let table = nlme.nib().neighbor_table();
        assert_eq!(table[0].relationship, 0x00);
    }

    #[test]
    fn join_sets_nwk_update_id_from_parent() {
        let mut mac = MockMlme::new();
        mac.expect_associate().returning(|_, _, _| {
            Ok(AssociationResponse {
                device_address: IeeeAddress(0),
                association_address: ShortAddress(0x1234),
                status: AssociationStatus::Successful,
            })
        });

        let (_guard, nlme) = make_nlme(mac);

        let mut n = make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0);
        n.update_id = 7;
        let mut table = StorageVec::new();
        table.push(n).unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(*nlme.nib().update_id(), 7);
    }

    #[test]
    fn join_fails_when_no_candidates() {
        let mac = MockMlme::new();
        let (_guard, nlme) = make_nlme(mac);
        nlme.nib()
            .update_neighbor_table(|value| *value = StorageVec::new());
        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::NotPermitted);
    }

    #[test]
    fn join_fails_when_already_joined() {
        let mac = MockMlme::new();
        let (_guard, nlme) = make_nlme(mac);
        nlme.nib().update_network_address(|value| *value = 0x0001);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::InvalidRequest);
    }

    #[test]
    fn join_skips_capacity_rejected_parent_tries_next() {
        let mut mac = MockMlme::new();
        let mut seq = mockall::Sequence::new();
        mac.expect_associate()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| {
                Ok(AssociationResponse {
                    device_address: IeeeAddress(0),
                    association_address: ShortAddress(0),
                    status: AssociationStatus::NetworkAtCapacity,
                })
            });
        mac.expect_associate()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| {
                Ok(AssociationResponse {
                    device_address: IeeeAddress(0),
                    association_address: ShortAddress(0x5678),
                    status: AssociationStatus::Successful,
                })
            });

        let (_guard, nlme) = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        table
            .push(make_neighbor(0xAAAA, 0x0001, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(confirm.network_address.0, 0x5678);

        let table = nlme.nib().neighbor_table();
        assert_eq!(table[0].potential_parent, 0);
        assert_eq!(table[1].relationship, 0x00);
    }

    #[test]
    fn join_all_candidates_rejected() {
        let mut mac = MockMlme::new();
        mac.expect_associate().returning(|_, _, _| {
            Ok(AssociationResponse {
                device_address: IeeeAddress(0),
                association_address: ShortAddress(0),
                status: AssociationStatus::AccessDenied,
            })
        });

        let (_guard, nlme) = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::PanAccessDenied);
        assert_eq!(confirm.network_address.0, 0xffff);
    }

    #[test]
    fn join_mac_error_reported() {
        let mut mac = MockMlme::new();
        mac.expect_associate()
            .returning(|_, _, _| Err(MacError::NoAck));

        let (_guard, nlme) = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib().update_neighbor_table(|value| *value = table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::MacError);
    }

    #[test]
    fn join_invalid_rejoin_network() {
        let mac = MockMlme::new();
        let (_guard, nlme) = make_nlme(mac);

        let mut req = default_join_request(0xDEAD);
        req.rejoin_network = RejoinNetwork::Orphan;

        let confirm = block_on(nlme.join(req));
        assert_eq!(confirm.status, NlmeJoinStatus::InvalidRequest);
    }

    // -------------------------------------------------------------------
    // NWK rejoin tests (§3.2.2.13.3, §3.4.6/.7)
    // -------------------------------------------------------------------

    fn rejoin_request(epid: u64) -> NlmeJoinRequest {
        NlmeJoinRequest {
            extended_pan_id: IeeeAddress(epid),
            rejoin_network: RejoinNetwork::NwkRejoin,
            capability_information: CapabilityInformation(0x80),
            security_enabled: true,
        }
    }

    /// Build the encrypted bytes of an inbound Rejoin Response command frame,
    /// as `poll_data` would deliver it from the parent.
    fn encrypted_rejoin_response(network_address: u16, status: u8) -> heapless::Vec<u8, 128> {
        let nib = nib::get_ref();
        let header = NwkHeader {
            frame_control: NwkFrameControl(0)
                .set_frame_type(NwkFrameType::NwkCommand)
                .set_protocol_version(2)
                .set_discover_route(DiscoverRoute::Suppress)
                .set_security_flag(true)
                .set_source_ieee_flag(true),
            destination: ShortAddress(*nib.network_address()),
            source: ShortAddress(0x0000),
            radius: 1,
            sequence_number: 0,
            destination_ieee: None,
            source_ieee: Some(*nib.ieee_address()),
            multicast_control: None,
            source_route_subframe: None,
        };
        let command = Command::RejoinResponse(RejoinResponse {
            network_address: ShortAddress(network_address),
            status,
        });
        let nwk_frame = NwkFrame::NwkCommand(NwkCommandFrame { header, command });
        let mut buf = heapless::Vec::<u8, 128>::new();
        buf.resize(128, 0).unwrap();
        let len = SecurityContext::get()
            .encrypt_nwk_frame_in_place(nwk_frame, &mut buf)
            .unwrap();
        buf.truncate(len);
        buf
    }

    #[test]
    fn nwk_rejoin_not_joined_is_invalid() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::InvalidRequest);
    }

    #[test]
    fn nwk_rejoin_without_security_is_sent_unsecured() {
        // §4.6.3.3.2: a Trust Center rejoin carries no NWK security
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .withf(|_dest, payload| {
                let (header, _) = NwkHeader::try_read(payload, ()).unwrap();
                !header.frame_control.security_flag()
            })
            .returning(|_, _| Ok(()));
        mac.expect_poll_data()
            .returning(|_, _| Err(MacError::NoData));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let mut req = rejoin_request(0xDEAD);
        req.security_enabled = false;

        let confirm = block_on(nlme.join(req));
        assert_eq!(confirm.status, NlmeJoinStatus::MacError);
    }

    #[test]
    fn nwk_rejoin_epid_mismatch_is_invalid() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.join(rejoin_request(0xBEEF)));
        assert_eq!(confirm.status, NlmeJoinStatus::InvalidRequest);
    }

    #[test]
    fn nwk_rejoin_success_sends_request_and_updates_address() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .withf(|dest, payload| {
                let mut buf = [0u8; 256];
                buf[..payload.len()].copy_from_slice(payload);
                let Ok(NwkFrame::NwkCommand(command_frame)) =
                    SecurityContext::get().decrypt_nwk_frame_in_place(&mut buf[..payload.len()])
                else {
                    return false;
                };
                let Command::RejoinRequest(request) = command_frame.command else {
                    return false;
                };
                *dest == Address::Short(PanId(0xAAAA), MacShortAddress(0x0000))
                    // rejoin_request() uses CapabilityInformation(0x80):
                    // allocate address set, device_type (end device) clear
                    && request.capability_information.device_type() == 0
                    && request.capability_information.allocate_address() == 1
            })
            .returning(|_, _| Ok(()));
        mac.expect_poll_data().times(1).returning(|_, buf| {
            let response = encrypted_rejoin_response(0x5678, 0x00);
            buf[..response.len()].copy_from_slice(&response);
            Ok((response.len(), 200u8))
        });
        // the assigned address has to reach the hardware filter
        mac.expect_set_short_address()
            .times(1)
            .withf(|address| *address == ShortAddress(0x5678))
            .returning(|_| ());

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(confirm.network_address, ShortAddress(0x5678));
        assert_eq!(*nlme.nib().network_address(), 0x5678);
    }

    #[test]
    fn rejoin_response_from_receive_path_applies_address_and_signals() {
        // a concurrent receive loop may retrieve the response instead of the
        // rejoin procedure's own poll
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        block_on(nlme.handle_nwk_command(
            &dummy_header(0x0000),
            Command::RejoinResponse(RejoinResponse {
                network_address: ShortAddress(0x5678),
                status: 0x00,
            }),
        ));

        assert_eq!(*nlme.nib().network_address(), 0x5678);
        block_on(nlme.rejoin_response.wait());
    }

    #[test]
    fn nwk_rejoin_network_at_capacity() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));
        mac.expect_poll_data().times(1).returning(|_, buf| {
            let response = encrypted_rejoin_response(0xffff, 0x01);
            buf[..response.len()].copy_from_slice(&response);
            Ok((response.len(), 200u8))
        });

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::PanAtCapacity);
    }

    #[test]
    fn nwk_rejoin_no_response_is_mac_error() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));
        mac.expect_poll_data()
            .returning(|_, _| Err(MacError::NoData));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::MacError);
    }

    #[test]
    fn nwk_rejoin_transmit_failure_is_mac_error() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .returning(|_, _| Err(MacError::NoAck));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::MacError);
    }

    // -------------------------------------------------------------------
    // end device timeout tests (§3.6.10)
    // -------------------------------------------------------------------

    /// A minimal NWK header for feeding `handle_nwk_command` in tests; the
    /// source address is the only field most handlers inspect.
    fn dummy_header(source: u16) -> NwkHeader<'static> {
        NwkHeader {
            frame_control: NwkFrameControl(0).set_frame_type(NwkFrameType::NwkCommand),
            destination: ShortAddress(0x0000),
            source: ShortAddress(source),
            radius: 1,
            sequence_number: 0,
            destination_ieee: None,
            source_ieee: None,
            multicast_control: None,
            source_route_subframe: None,
        }
    }

    /// Seed the global NIB with a joined-network state: parent neighbor,
    /// addresses, and NWK security material.
    fn seed_joined_nib(nlme: &Nlme<MockMlme>) {
        use zigbee_types::ByteArray;

        use crate::nwk::nib::NetworkSecurityMaterialDescriptor;

        let nib = nlme.nib();
        nib.update_panid(|value| *value = 0xAAAA);
        nib.update_network_address(|value| *value = 0x1234);
        nib.update_extended_panid(|value| *value = 0xDEAD);

        let mut table = StorageVec::new();
        let mut parent = make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0);
        parent.relationship = relationship::PARENT;
        table.push(parent).unwrap();
        nib.update_neighbor_table(|value| *value = table);

        let mut set = StorageVec::new();
        set.push(NetworkSecurityMaterialDescriptor {
            key_seq_number: 0,
            outgoing_frame_counter: 1,
            incoming_frame_counter_set: StorageVec::new(),
            key: ByteArray([0x42; 16]),
            network_key_type: 0,
        })
        .unwrap();
        nib.update_security_material_set(|value| *value = set);
    }

    #[test]
    fn send_end_device_timeout_request_encodes_command() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .withf(|dest, payload| {
                assert_eq!(
                    *dest,
                    Address::Short(PanId(0xAAAA), MacShortAddress(0x0000))
                );
                // header is unencrypted; verify it directly
                let (header, _) = NwkHeader::try_read(payload, ()).unwrap();
                assert_eq!(header.frame_control.frame_type(), NwkFrameType::NwkCommand);
                assert!(header.frame_control.security_flag());
                assert!(header.frame_control.source_ieee_flag());
                assert_eq!(header.radius, 1);
                assert_eq!(header.destination, ShortAddress(0x0000));
                assert_eq!(header.source, ShortAddress(0x1234));

                // decrypt in place to verify the command payload
                let mut buf = [0u8; 256];
                buf[..payload.len()].copy_from_slice(payload);
                let frame = SecurityContext::get()
                    .decrypt_nwk_frame_in_place(&mut buf[..payload.len()])
                    .unwrap();
                let NwkFrame::NwkCommand(command_frame) = frame else {
                    panic!("expected NWK command frame");
                };
                let Command::EndDeviceTimeoutRequest(request) = command_frame.command else {
                    panic!("expected end device timeout request");
                };
                assert_eq!(request.requested_timeout, 0x08);
                assert_eq!(request.end_device_configuration, 0x00);
                true
            })
            .returning(|_, _| Ok(()));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        block_on(nlme.send_end_device_timeout_request()).unwrap();
        assert_eq!(
            nlme.pending_timeout_request.load(Ordering::Relaxed),
            *nlme.nib().end_device_timeout_default()
        );
    }

    #[test]
    fn send_end_device_timeout_request_not_joined() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        let result = block_on(nlme.send_end_device_timeout_request());
        assert!(matches!(result, Err(NetworkError::NotJoined)));
    }

    #[test]
    fn end_device_timeout_response_success_updates_nib() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        nlme.pending_timeout_request.store(0x03, Ordering::Relaxed);

        block_on(nlme.handle_nwk_command(
            &dummy_header(0x0000),
            Command::EndDeviceTimeoutResponse(EndDeviceTimeoutResponse {
                status: EndDeviceTimeoutResponse::STATUS_SUCCESS,
                parent_information: 0b011,
            }),
        ));

        let nib = nlme.nib();
        assert_eq!(*nib.parent_information(), 0b011);
        assert_eq!(*nib.end_device_timeout(), 0x03);
        // pending request consumed
        assert_eq!(
            nlme.pending_timeout_request.load(Ordering::Relaxed),
            NO_PENDING_TIMEOUT
        );
    }

    #[test]
    fn end_device_timeout_response_rejected_leaves_nib_untouched() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        nlme.pending_timeout_request.store(0x03, Ordering::Relaxed);

        block_on(nlme.handle_nwk_command(
            &dummy_header(0x0000),
            Command::EndDeviceTimeoutResponse(EndDeviceTimeoutResponse {
                status: EndDeviceTimeoutResponse::STATUS_INCORRECT_VALUE,
                parent_information: 0b001,
            }),
        ));

        let nib = nlme.nib();
        assert_eq!(*nib.parent_information(), 0x00);
        assert_eq!(*nib.end_device_timeout(), NO_PENDING_TIMEOUT);
    }

    #[test]
    fn end_device_timeout_response_unsolicited_ignored() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        block_on(nlme.handle_nwk_command(
            &dummy_header(0x0000),
            Command::EndDeviceTimeoutResponse(EndDeviceTimeoutResponse {
                status: EndDeviceTimeoutResponse::STATUS_SUCCESS,
                parent_information: 0b001,
            }),
        ));

        let nib = nlme.nib();
        assert_eq!(*nib.parent_information(), 0x00);
        assert_eq!(*nib.end_device_timeout(), NO_PENDING_TIMEOUT);
    }

    // -------------------------------------------------------------------
    // leave tests (§3.2.2.18, §3.6.1.10)
    // -------------------------------------------------------------------

    #[test]
    fn leave_not_joined_is_invalid() {
        let (_guard, nlme) = make_nlme(MockMlme::new());

        let confirm = block_on(nlme.leave(NlmeLeaveRequest {
            device_address: None,
            remove_children: false,
            rejoin: false,
        }));
        assert_eq!(confirm.status, NlmeLeaveStatus::InvalidRequest);
    }

    #[test]
    fn leave_other_device_is_unknown() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.leave(NlmeLeaveRequest {
            device_address: Some(IeeeAddress(0x1111_1111_1111_1111)),
            remove_children: false,
            rejoin: false,
        }));
        assert_eq!(confirm.status, NlmeLeaveStatus::UnknownDevice);
    }

    #[test]
    fn leave_self_end_device_unicasts_to_parent_and_clears_nib() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .withf(|dest, payload| {
                assert_eq!(
                    *dest,
                    Address::Short(PanId(0xAAAA), MacShortAddress(0x0000))
                );
                let mut buf = [0u8; 256];
                buf[..payload.len()].copy_from_slice(payload);
                let frame = SecurityContext::get()
                    .decrypt_nwk_frame_in_place(&mut buf[..payload.len()])
                    .unwrap();
                let NwkFrame::NwkCommand(command_frame) = frame else {
                    panic!("expected NWK command frame");
                };
                let Command::Leave(leave) = command_frame.command else {
                    panic!("expected leave command");
                };
                assert!(!leave.command_options.request());
                assert!(!leave.command_options.rejoin());
                assert!(!leave.command_options.remove_children());
                true
            })
            .returning(|_, _| Ok(()));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        // §3.6.1.10.1: an end device sends the remove children sub-field as 0
        // even when the request asked for it
        let confirm = block_on(nlme.leave(NlmeLeaveRequest {
            device_address: None,
            remove_children: true,
            rejoin: false,
        }));
        assert_eq!(confirm.status, NlmeLeaveStatus::Success);
        assert_eq!(confirm.device_address, None);

        let nib = nlme.nib();
        assert_eq!(*nib.network_address(), 0xffff);
        assert_eq!(*nib.extended_panid(), 0);
        assert!(nib.neighbor_table().is_empty());
        assert!(nib.security_material_set().is_empty());
    }

    #[test]
    fn leave_self_clears_nib_even_when_transmit_fails() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .returning(|_, _| Err(MacError::NoAck));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.leave(NlmeLeaveRequest {
            device_address: None,
            remove_children: false,
            rejoin: false,
        }));
        // §3.6.1.10.1: the local leave runs regardless of the confirm status
        assert_eq!(confirm.status, NlmeLeaveStatus::MacError);
        assert_eq!(*nlme.nib().network_address(), NWK_UNASSIGNED_ADDRESS);
        assert!(nlme.nib().security_material_set().is_empty());
    }

    #[test]
    fn leave_request_to_broadcast_address_is_dropped() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        let header = NwkHeader {
            destination: ShortAddress(0xfffd),
            ..dummy_header(0x0000)
        };
        let leave = Leave {
            command_options: LeaveCommandOptions(0).set_request(true),
        };
        // no transmit_data expectation set: a call here would panic the mock
        block_on(nlme.handle_nwk_command(&header, Command::Leave(leave)));

        assert_eq!(*nlme.nib().network_address(), 0x1234);
    }

    #[test]
    fn leave_notification_from_parent_raises_indication() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        let leave = Leave {
            command_options: LeaveCommandOptions(0).set_rejoin(true),
        };
        block_on(nlme.handle_nwk_command(&dummy_header(0x0000), Command::Leave(leave)));

        // §3.6.1.10.3: removed by the parent -> NULL device address
        assert_eq!(
            nlme.take_leave_indication(),
            Some(NlmeLeaveIndication {
                device_address: None,
                rejoin: true,
            })
        );
    }

    #[test]
    fn leave_self_with_rejoin_leaves_nib_untouched() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.leave(NlmeLeaveRequest {
            device_address: None,
            remove_children: false,
            rejoin: true,
        }));
        assert_eq!(confirm.status, NlmeLeaveStatus::Success);

        let nib = nlme.nib();
        // rejoin requested: the NIB is left intact for the (not yet
        // implemented) rejoin procedure, aside from the extended PAN id
        // which §3.6.1.10.1 clears unconditionally for an end device.
        assert_eq!(*nib.network_address(), 0x1234);
        assert_eq!(*nib.extended_panid(), 0);
        assert!(!nib.neighbor_table().is_empty());
    }

    #[test]
    fn leave_notification_removes_only_the_notifying_neighbor() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);
        nlme.nib().update_neighbor_table(|table| {
            let _ = table.push(make_neighbor(0xAAAA, 0x5678, 0xBEEF, 200, 1));
        });
        assert_eq!(nlme.nib().neighbor_table().len(), 2);

        let leave = Leave {
            command_options: LeaveCommandOptions(0),
        };
        block_on(nlme.handle_nwk_command(&dummy_header(0x5678), Command::Leave(leave)));

        let nib = nlme.nib();
        assert_eq!(nib.neighbor_table().len(), 1);
        assert_eq!(
            nib.neighbor_table()[0].network_address,
            ShortAddress(0x0000)
        );
        // not our parent -> we are still joined
        assert_ne!(*nib.network_address(), 0xffff);
    }

    #[test]
    fn leave_notification_from_parent_marks_device_removed() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        let leave = Leave {
            command_options: LeaveCommandOptions(0),
        };
        block_on(nlme.handle_nwk_command(&dummy_header(0x0000), Command::Leave(leave)));

        let nib = nlme.nib();
        assert_eq!(*nib.extended_panid(), 0);
        assert!(nib.neighbor_table().is_empty());
    }

    #[test]
    fn leave_request_from_parent_triggers_self_removal() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let leave = Leave {
            command_options: LeaveCommandOptions(0).set_request(true),
        };
        block_on(nlme.handle_nwk_command(&dummy_header(0x0000), Command::Leave(leave)));

        assert_eq!(*nlme.nib().network_address(), 0xffff);
    }

    #[test]
    fn leave_request_from_non_parent_is_ignored() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);

        let leave = Leave {
            command_options: LeaveCommandOptions(0).set_request(true),
        };
        // no transmit_data expectation set: a call here would panic the mock
        block_on(nlme.handle_nwk_command(&dummy_header(0x9999), Command::Leave(leave)));

        assert_eq!(*nlme.nib().network_address(), 0x1234);
    }

    #[test]
    fn leave_request_dropped_for_coordinator() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);
        nlme.nib().update_network_address(|value| *value = 0x0000);

        let leave = Leave {
            command_options: LeaveCommandOptions(0).set_request(true),
        };
        // no transmit_data expectation set: a call here would panic the mock
        block_on(nlme.handle_nwk_command(&dummy_header(0x0000), Command::Leave(leave)));

        assert_eq!(*nlme.nib().network_address(), 0x0000);
    }

    #[test]
    fn negotiated_poll_interval() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        let nib = nlme.nib();

        // not negotiated
        assert_eq!(nlme.negotiated_poll_interval_ms(), None);

        // negotiated 2 min with MAC data poll keepalive: 120 s / 3 = 40 s
        nib.update_end_device_timeout(|value| *value = 1);
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::MAC_DATA_POLL_KEEPALIVE;
        });
        assert_eq!(nlme.negotiated_poll_interval_ms(), Some(40_000));

        // parent without MAC data poll keepalive support
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE;
        });
        assert_eq!(nlme.negotiated_poll_interval_ms(), None);
    }
}
