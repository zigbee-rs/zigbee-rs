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
use core::sync::atomic::AtomicU32;
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
use management::NlmeNwkStatusIndication;
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
use zigbee_mac::mlme::MacConfig;
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
use crate::nwk::frame::command::network_status::NetworkStatusCode;
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
// Response (3.4.7.1: indirect transmission for a sleepy child)
const REJOIN_RESPONSE_POLL_RETRIES: u8 = 20;

// poll attempts covering macResponseWaitTime while waiting for the keepalive's
// End Device Timeout Response (3.6.10.3)
const TIMEOUT_RESPONSE_POLL_RETRIES: u8 = 3;

// `parent_timeout_remaining_ms` value meaning "not tracking": either no
// timeout was negotiated or it already expired
const PARENT_TIMEOUT_INACTIVE: u32 = 0;

// consecutive transmit failures classifying the link to the parent as failed
// (3.6.3.7 leaves the counting scheme to the implementer)
const MAX_PARENT_TRANSMIT_FAILURES: u8 = 3;

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
    #[error("frame does not fit a single APS frame")]
    FrameTooLong,
    #[error("no APS acknowledgement received")]
    AckTimeout,
    #[error("parent link failure")]
    ParentLinkFailure,
    #[error("security error: {0}")]
    SecurityError(#[from] crate::security::SecurityError),
}

impl From<byte::Error> for NetworkError {
    fn from(_: byte::Error) -> Self {
        Self::ParseError
    }
}

/// Network Layer Management Entity (3.2.2).
///
/// Provides the management service access point (NLME-SAP) that allows
/// the next higher layer to interact with the NWK layer: network
/// discovery, formation, joining, rejoining, data transmission, etc.
pub struct Nlme<M> {
    mac: M,
    nwk_seq: AtomicU8,
    // requested timeout enum awaiting a response (3.6.10.2); 0xff = none
    pending_timeout_request: AtomicU8,
    // remaining time before the parent is assumed to have aged this device out
    // (3.6.10.6); the parent's neighbor entry is the only one an end device
    // tracks, and the counter is volatile, so it lives here rather than in the
    // flash-backed neighbor table
    parent_timeout_remaining_ms: AtomicU32,
    // Rejoin Response (3.4.7) handed from the receive path to the rejoin
    // procedure waiting for it
    rejoin_response: Signal<RejoinResponse>,
    // End Device Timeout Response (3.4.12) handed from the receive path to the
    // keepalive waiting for it
    timeout_response: Signal<EndDeviceTimeoutResponse>,
    // NLME-LEAVE.indication (3.2.2.19) raised by the receive path for the
    // higher layer, which decides whether to re-commission or rejoin
    leave_indication: Signal<NlmeLeaveIndication>,
    // NLME-NWK-STATUS.indication (3.2.2.32) reporting a network failure to the
    // higher layer
    nwk_status_indication: Signal<NlmeNwkStatusIndication>,
}

/// Keepalive method negotiated with the router parent (3.6.10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepaliveMethod {
    /// The parent tracks a MAC data poll as a keepalive
    /// (`nwkParentInformation` bit 0).
    MacDataPoll,
    /// The parent expects an End Device Timeout Request
    /// (`nwkParentInformation` bit 1).
    TimeoutRequest,
    /// No timeout was negotiated: the parent ages this device out on its own
    /// terms and no keepalive is sent.
    None,
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
            parent_timeout_remaining_ms: AtomicU32::new(PARENT_TIMEOUT_INACTIVE),
            rejoin_response: Signal::new(),
            timeout_response: Signal::new(),
            leave_indication: Signal::new(),
            nwk_status_indication: Signal::new(),
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

    /// 3.2.2.32 - take a pending NLME-NWK-STATUS.indication, if any.
    ///
    /// Reports a network failure to the higher layer; the indication stays
    /// pending until taken. A status of
    /// [`NetworkStatusCode::ParentLinkFailure`] means the link to the parent
    /// is gone and the device should rejoin (3.6.10.10).
    pub fn take_nwk_status_indication(&self) -> Option<NlmeNwkStatusIndication> {
        self.nwk_status_indication.try_take()
    }

    /// 3.2.2.32 - wait for the next NLME-NWK-STATUS.indication.
    pub async fn wait_nwk_status_indication(&self) -> NlmeNwkStatusIndication {
        self.nwk_status_indication.wait().await
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
    /// (3.3.2).
    ///
    /// When `secure` is false the frame is sent in the clear, as required for
    /// a Trust Center rejoin (4.6.3.3.2).
    fn build_nwk_command_frame(
        &self,
        destination: ShortAddress,
        command: Command<'_>,
        secure: bool,
        buf: &mut [u8],
    ) -> Result<usize, NetworkError> {
        let nib = self.nib();
        // the destination IEEE address is carried whenever it is known
        // (3.4.6.2, 3.4.11.2)
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

    /// Build a failed NLME-JOIN.confirm (3.2.2.13.3).
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

    /// Send an End Device Timeout Request to the parent (3.6.10.2).
    ///
    /// Requests the timeout enumeration from `nwkEndDeviceTimeoutDefault`.
    /// The parent's response is consumed by the receive path and updates
    /// `nwkParentInformation` and the negotiated timeout in the NIB; no
    /// response means a legacy parent and leaves the NIB untouched.
    pub async fn send_end_device_timeout_request(&self) -> Result<(), NetworkError> {
        let requested_timeout = *self.nib().end_device_timeout_default();
        let command = Command::EndDeviceTimeoutRequest(EndDeviceTimeoutRequest {
            requested_timeout,
            // all bits reserved, must be 0 (3.4.11.3.2)
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

    /// Program the MAC from the restored information base and report whether
    /// this device is resuming on a network (3.6.8).
    ///
    /// A device that was reset while joined keeps its addresses in the NIB but
    /// the radio comes up unconfigured: the PAN id, short address, channel and
    /// the filtering that goes with membership have to be applied before it
    /// can poll its parent again. A device that is not on a network only gets
    /// the channel it will look for one on, leaving the rest to the join.
    pub async fn resume(&self, channel: u8) -> bool {
        let nib = self.nib();
        let network_address = *nib.network_address();
        let on_a_network = network_address != NWK_UNASSIGNED_ADDRESS;

        let config = if on_a_network {
            MacConfig::joined(channel, PanId(*nib.panid()), ShortAddress(network_address))
        } else {
            MacConfig::channel(channel)
        };
        self.mac.configure(config).await;

        on_a_network
    }

    /// Negotiate the end-device timeout with the parent (3.6.10.2).
    ///
    /// Sends the request and waits for the parent's response, which stores the
    /// negotiated timeout and the parent's keepalive methods in the NIB.
    /// Returns whether a timeout was negotiated: a legacy parent never answers
    /// and keeps its own default, leaving this device without a keepalive
    /// method.
    pub async fn negotiate_end_device_timeout(&self) -> Result<bool, NetworkError> {
        // arm before transmitting: the response may be delivered by a
        // concurrently running receive loop
        self.timeout_response.reset();
        self.send_end_device_timeout_request().await?;

        let response = with_timeout(
            self.timeout_response.wait(),
            self.poll_until_timeout_response(TIMEOUT_RESPONSE_POLL_RETRIES),
        )
        .await
        .or_else(|| self.timeout_response.try_take());

        match response {
            Some(response) if response.status == EndDeviceTimeoutResponse::STATUS_SUCCESS => {
                Ok(true)
            }
            Some(response) => {
                log::warn!(
                    "[NWK] end-device timeout rejected (status={:#04x})",
                    response.status
                );
                Ok(false)
            }
            None => {
                log::warn!("[NWK] no end-device timeout response, assuming legacy parent");
                Ok(false)
            }
        }
    }

    /// Recommended maximum poll interval in milliseconds, allowing three
    /// keepalives per negotiated timeout period (3.6.10.3).
    ///
    /// Returns `None` when no timeout was negotiated or the parent does not
    /// support the MAC data poll keepalive method.
    pub fn negotiated_poll_interval_ms(&self) -> Option<u32> {
        if self.keepalive_method() != KeepaliveMethod::MacDataPoll {
            return None;
        }
        self.keepalive_interval_ms()
    }

    /// Keepalive method the parent expects (3.6.10.3).
    ///
    /// Bit 0 of `nwkParentInformation` takes precedence over bit 1; without a
    /// negotiated timeout no keepalive is sent.
    pub fn keepalive_method(&self) -> KeepaliveMethod {
        let nib = self.nib();
        if end_device_timeout_request::timeout_seconds(*nib.end_device_timeout()).is_none() {
            return KeepaliveMethod::None;
        }
        let parent_information = *nib.parent_information();
        if parent_information & EndDeviceTimeoutResponse::MAC_DATA_POLL_KEEPALIVE != 0 {
            KeepaliveMethod::MacDataPoll
        } else if parent_information & EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE != 0 {
            KeepaliveMethod::TimeoutRequest
        } else {
            KeepaliveMethod::None
        }
    }

    /// Recommended interval between keepalives in milliseconds: three
    /// keepalives per negotiated Device Timeout period (3.6.10.3).
    ///
    /// Returns `None` when no timeout was negotiated with the parent.
    pub fn keepalive_interval_ms(&self) -> Option<u32> {
        Some(self.negotiated_timeout_ms()? / 3)
    }

    // negotiated Device Timeout period in milliseconds (Table 3.52)
    fn negotiated_timeout_ms(&self) -> Option<u32> {
        let seconds =
            end_device_timeout_request::timeout_seconds(*self.nib().end_device_timeout())?;
        Some(seconds.saturating_mul(1000))
    }

    /// Send one keepalive to the parent (3.6.10.3).
    ///
    /// The method follows `nwkParentInformation`: a MAC data poll, or an End
    /// Device Timeout Request whose response is awaited. A failed timeout
    /// request raises an NLME-NWK-STATUS.indication with
    /// [`NetworkStatusCode::ParentLinkFailure`] and returns
    /// [`NetworkError::ParentLinkFailure`]; a successful keepalive restarts
    /// the local timeout (3.6.10.6).
    pub async fn send_keepalive(&self) -> Result<(), NetworkError> {
        match self.keepalive_method() {
            KeepaliveMethod::MacDataPoll => self.mac_data_poll_keepalive().await,
            KeepaliveMethod::TimeoutRequest => self.timeout_request_keepalive().await,
            KeepaliveMethod::None => Ok(()),
        }
    }

    // 3.6.10.3: a data poll refreshes the parent's timeout on its own; any
    // frame it retrieves belongs to the receive loop, which is not running
    // when the higher layer drives the keepalive itself
    async fn mac_data_poll_keepalive(&self) -> Result<(), NetworkError> {
        let mut buf = [0u8; 256];
        match self.poll_nwk_frame(&mut buf).await {
            Ok(Some(_)) => log::debug!("[NWK] keepalive poll dropped a data frame"),
            Ok(None) => (),
            Err(e) => return Err(e),
        }
        self.refresh_parent_timeout();
        Ok(())
    }

    // 3.6.10.3: unicast an End Device Timeout Request and wait
    // macResponseWaitTime for the response; anything else is a parent link
    // failure
    async fn timeout_request_keepalive(&self) -> Result<(), NetworkError> {
        // arm before transmitting: the response may be delivered by a
        // concurrently running receive loop
        self.timeout_response.reset();
        if let Err(e) = self.send_end_device_timeout_request().await {
            log::warn!("[NWK] keepalive transmission failed ({e:?})");
            return Err(self.parent_link_failure());
        }

        let response = with_timeout(
            self.timeout_response.wait(),
            self.poll_until_timeout_response(TIMEOUT_RESPONSE_POLL_RETRIES),
        )
        .await
        // the response may have arrived in the very poll that ended the polling
        .or_else(|| self.timeout_response.try_take());

        match response {
            Some(response) if response.status == EndDeviceTimeoutResponse::STATUS_SUCCESS => {
                self.refresh_parent_timeout();
                Ok(())
            }
            Some(response) => {
                log::warn!("[NWK] keepalive rejected (status={:#04x})", response.status);
                Err(self.parent_link_failure())
            }
            None => {
                log::warn!("[NWK] no keepalive response from parent");
                Err(self.parent_link_failure())
            }
        }
    }

    // poll the parent until the End Device Timeout Response has been processed
    // or `retries` polls went unanswered; a sleepy child only receives the
    // response by polling for it
    async fn poll_until_timeout_response(&self, retries: u8) {
        let mut buf = [0u8; 128];
        for _ in 0..retries {
            if self.timeout_response.is_signaled() {
                return;
            }
            match self.poll_parent(&mut buf).await {
                Ok(Some(len)) => match self.process_received_nwk_frame(&mut buf[..len]).await {
                    Ok(Some(_)) => log::debug!("[NWK] keepalive poll dropped a data frame"),
                    Ok(None) => (),
                    Err(e) => log::debug!("[NWK] keepalive frame not processed: {e:?}"),
                },
                Ok(None) => (),
                Err(e) => log::debug!("[NWK] keepalive poll failed: {e:?}"),
            }
        }
    }

    /// Restart the local end-device timeout after evidence that the parent
    /// still holds this device in its neighbor table (3.6.10.6).
    pub fn refresh_parent_timeout(&self) {
        let remaining = self
            .negotiated_timeout_ms()
            .unwrap_or(PARENT_TIMEOUT_INACTIVE);
        self.parent_timeout_remaining_ms
            .store(remaining, Ordering::Relaxed);
    }

    /// Advance the local end-device timeout by `elapsed_ms` (3.6.10.6).
    ///
    /// Returns `true` once the timeout expires: the parent is assumed to have
    /// aged this device out, which is reported to the higher layer as an
    /// NLME-NWK-STATUS.indication with
    /// [`NetworkStatusCode::ParentLinkFailure`]. Tracking then stops until the
    /// next successful keepalive.
    pub fn tick_parent_timeout(&self, elapsed_ms: u32) -> bool {
        let remaining = self.parent_timeout_remaining_ms.load(Ordering::Relaxed);
        if remaining == PARENT_TIMEOUT_INACTIVE {
            return false;
        }
        let remaining = remaining.saturating_sub(elapsed_ms.max(1));
        self.parent_timeout_remaining_ms
            .store(remaining, Ordering::Relaxed);
        if remaining != PARENT_TIMEOUT_INACTIVE {
            return false;
        }
        log::warn!("[NWK] end-device timeout elapsed, assuming aged out by parent");
        self.parent_link_failure();
        true
    }

    /// Transmit a built frame through the parent, tracking the link
    /// (3.6.3.7).
    ///
    /// Every neighbor with an outgoing link carries a failure counter; once
    /// the link to the parent counts as failed, the higher layer is informed
    /// with an NLME-NWK-STATUS.indication of `ParentLinkFailure` (3.6.3.7.1)
    /// and [`NetworkError::ParentLinkFailure`] is returned.
    async fn transmit_via_parent(&self, buf: &[u8]) -> Result<(), NetworkError> {
        let parent = self.parent_address()?;
        let Err(e) = self.mac.transmit_data(parent, buf).await else {
            // an acknowledged frame proves the parent still holds this device
            // in its neighbor table (3.6.10.6)
            self.update_parent_neighbor(|neighbor| neighbor.transmit_failure = 0);
            self.refresh_parent_timeout();
            return Ok(());
        };

        let mut failures = 0;
        self.update_parent_neighbor(|neighbor| {
            neighbor.transmit_failure = neighbor.transmit_failure.saturating_add(1);
            failures = neighbor.transmit_failure;
        });
        log::debug!("[NWK] transmission via parent failed ({e:?}), failures={failures}");
        if failures < MAX_PARENT_TRANSMIT_FAILURES {
            return Err(e.into());
        }

        self.update_parent_neighbor(|neighbor| neighbor.transmit_failure = 0);
        Err(self.parent_link_failure())
    }

    // apply `update` to the parent's neighbor table entry, if there is one
    fn update_parent_neighbor(&self, update: impl FnOnce(&mut NwkNeighbor)) {
        self.nib().update_neighbor_table(|table| {
            if let Some(parent) = table
                .iter_mut()
                .find(|n| n.relationship == relationship::PARENT)
            {
                update(parent);
            }
        });
    }

    // raise NLME-NWK-STATUS.indication 0x09 (3.6.3.7.1, 3.6.10.3, 3.2.2.32)
    fn parent_link_failure(&self) -> NetworkError {
        let network_address = self
            .parent_short_address()
            .unwrap_or(ShortAddress(NWK_UNASSIGNED_ADDRESS));
        self.nwk_status_indication.signal(NlmeNwkStatusIndication {
            status: NetworkStatusCode::ParentLinkFailure,
            network_address,
        });
        NetworkError::ParentLinkFailure
    }

    /// Returns a reference to the global NIB singleton.
    pub fn nib(&self) -> &'static Nib {
        nib::get_ref()
    }

    /// Select parent candidates for an association join (3.6.1.4.1.1).
    fn select_parent_candidates(
        &self,
        extended_pan_id: IeeeAddress,
        join_as_router: bool,
    ) -> heapless::Vec<usize, 16> {
        self.select_candidates(extended_pan_id, join_as_router, true)
    }

    /// Select parent candidates for a rejoin (3.6.1.4.2).
    ///
    /// Same selection as for a join except that the network need not permit
    /// joining: a rejoin reconnects a device the network already knows.
    fn select_rejoin_candidates(
        &self,
        extended_pan_id: IeeeAddress,
        join_as_router: bool,
    ) -> heapless::Vec<usize, 16> {
        self.select_candidates(extended_pan_id, join_as_router, false)
    }

    fn select_candidates(
        &self,
        extended_pan_id: IeeeAddress,
        join_as_router: bool,
        require_permit_joining: bool,
    ) -> heapless::Vec<usize, 16> {
        let table = self.nib().neighbor_table();
        log::debug!("[NWK-JOIN] neighbor table: {table:#?}");
        let stack_profile = self.nib().stack_profile();
        let permits = |n: &NwkNeighbor| !require_permit_joining || n.permit_joining;

        // 3.6.1.4.1.1: the update id wraps back to zero, so it is treated as
        // a modular counter - an id `b` is newer than `a` when the signed
        // difference `(b - a) as i8` is positive
        let mut matching = table.iter().filter(|n| {
            n.extended_pan_id == extended_pan_id && permits(n) && n.potential_parent == 1
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

        let mut candidates: heapless::Vec<usize, 16> = table
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                // correct network, accepting joins of the right type, low
                // enough link cost, a potential parent, and on the most
                // recent update id
                n.extended_pan_id == extended_pan_id
                    && permits(n)
                    && if join_as_router {
                        n.router_capacity
                    } else {
                        n.end_device_capacity
                    }
                    && link_cost_from_lqi(n.lqi) <= MAX_PARENT_LINK_COST
                    && n.potential_parent == 1
                    && n.update_id == best_update_id
            })
            .map(|(i, _)| i)
            .collect();

        // nwkStackProfile == 1 prefers minimum depth (3.6.1.4.1.1)
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
    /// fresh join invalidates both (3.6.1.4.1.1, 3.6.10.2).
    fn reset_parent_negotiation(&self) {
        let nib = self.nib();
        nib.update_parent_information(|value| *value = 0);
        nib.update_end_device_timeout(|value| *value = NO_PENDING_TIMEOUT);
        self.parent_timeout_remaining_ms
            .store(PARENT_TIMEOUT_INACTIVE, Ordering::Relaxed);
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

    /// The network address of a neighbor whose IEEE address has been learned
    /// from a received frame (3.4.6.2).
    pub(crate) fn short_address_for_ieee(&self, ieee: IeeeAddress) -> Option<ShortAddress> {
        self.nib()
            .neighbor_table()
            .iter()
            .find(|n| n.extended_address == ieee)
            .map(|n| n.network_address)
    }

    /// Record the IEEE address a received frame reported for its source
    /// (3.4.6.2: it lets us address later commands by both addresses).
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

    /// Issue one MLME-POLL to the parent (3.6.6).
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

    /// Poll the parent once (MLME-POLL, 3.6.6) and process the retrieved NWK
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
            // "no data" so the APS/ZDO caller skips this frame
            NwkFrame::NwkCommand(command_frame) => {
                self.learn_neighbor_ieee_address(&command_frame.header);
                self.handle_nwk_command(&command_frame.header, command_frame.command)
                    .await;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Process an inbound NWK command frame (3.4).
    ///
    /// Scaffolding extension point: every NWK command variant is matched so a
    /// handler can be filled in. Most are not yet acted upon — they are logged
    /// and ignored. Returning unit keeps the caller's receive loop simple; add
    /// a result type here if a handler needs to surface failures.
    async fn handle_nwk_command(&self, header: &NwkHeader<'_>, command: Command<'_>) {
        match command {
            // routing (3.4.1-3.4.2): a sleepy end device routes via its parent,
            // so route discovery is currently a no-op
            Command::RouteRequest(_) => log::trace!("[NWK] route request (ignored)"),
            Command::RouteReply(_) => log::trace!("[NWK] route reply (ignored)"),
            Command::RouteRecord(_) => log::trace!("[NWK] route record (ignored)"),
            // network maintenance (3.4.3, 3.4.9, 3.4.10)
            Command::NetworkStatus(_) => log::trace!("[NWK] network status (ignored)"),
            Command::NetworkReport(_) => log::trace!("[NWK] network report (ignored)"),
            Command::NetworkUpdate(_) => log::trace!("[NWK] network update (ignored)"),
            Command::Leave(leave) => self.handle_leave_command(header, &leave).await,
            // rejoin (3.4.6–3.4.7): a request is parent-side only; a response
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
                    // 3.4.7.3.1: the parent may hand out a different address
                    // than the one the device rejoined with
                    self.nib()
                        .update_network_address(|value| *value = response.network_address.0);
                }
                self.rejoin_response.signal(response);
            }
            // link management (3.4.8, 3.4.13)
            Command::LinkStatus(_) => log::trace!("[NWK] link status (ignored)"),
            Command::LinkPowerDelta(_) => log::trace!("[NWK] link power delta (ignored)"),
            // end-device timeout requests are parent-side only (3.4.11)
            Command::EndDeviceTimeoutRequest(_) => {
                log::trace!("[NWK] end-device timeout request (ignored)");
            }
            // 3.6.10.2: on success store the parent information bitmask and
            // the negotiated timeout, otherwise the parent keeps its default
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
                    // the parent restarted its timeout counter for this device
                    // (3.6.10.5), so the local one follows (3.6.10.6)
                    self.refresh_parent_timeout();
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
                // hand it to a keepalive waiting for it (3.6.10.3)
                self.timeout_response.signal(response);
            }
            Command::Reserved(id) => log::trace!("[NWK] reserved command {id:#04x} (ignored)"),
        }
    }

    /// Process an inbound Leave command frame (3.6.1.10.3).
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
            // it is no longer a neighbor regardless of the rejoin flag
            self.nib().update_neighbor_table(|table| {
                table.retain(|n| n.network_address != header.source);
            });

            if is_from_parent {
                // our parent has dropped us: we are no longer on the network,
                // and the indication carries a NULL device address (3.6.1.10.3)
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

        // request == 1: someone is asking us to leave (3.6.1.10.3.1). A leave
        // request is sent by the parent itself, so its NWK source address is
        // the sender the spec tests
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

        // 3.2.2.19: removed by another device, so the indication carries a NULL
        // device address; with the rejoin flag it asks the higher layer to come
        // back onto the network (3.6.1.10.4)
        self.leave_indication.signal(NlmeLeaveIndication {
            device_address: None,
            rejoin: options.rejoin(),
        });
    }

    /// Validate a leave request against 3.6.1.10.3.1, which governs both the
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

    /// 3.6.1.10.1 + 3.6.1.10.4: announce this device's own removal from the
    /// network, then apply the local leave procedure.
    ///
    /// The local leave runs whether or not the announcement made it onto the
    /// air; the transmit result only feeds the NLME-LEAVE.confirm status.
    async fn leave_network(&self, remove_children: bool, rejoin: bool) -> Result<(), NetworkError> {
        let result = self.announce_leave(remove_children, rejoin).await;
        self.local_leave(rejoin);
        result
    }

    /// 3.6.1.10.1: transmit this device's own leave command frame, with the
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
            // read out before the `.await` below — held across it, this
            // guard would deadlock against a concurrent write of this field
            let pan_id = *self.nib().panid();
            self.mac
                .transmit_data(
                    Address::Short(PanId(pan_id), MacShortAddress(NWK_BROADCAST_ALL)),
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
        // 3.6.1.10.1: an end device clears the extended PAN id right after
        // transmitting, ahead of the rest of the local leave process
        self.nib().update_extended_panid(|value| *value = 0);
        result.map_err(Into::into)
    }

    /// 3.6.1.10.4: the local half of the leave procedure.
    fn local_leave(&self, rejoin: bool) {
        if rejoin {
            // step 1: with Rejoin set the NIB is kept, so the remembered
            // network is still there for a later NLME-JOIN.request with
            // `RejoinNetwork::NwkRejoin`; driving that is the higher layer's
            // call, which the NLME-LEAVE.indication/confirm hands it
            log::info!("[NWK] left the network, rejoin requested");
            return;
        }

        self.clear_nib_on_leave();
    }

    /// 3.6.1.10.4 (Rejoin = FALSE): clear the NIB attributes describing
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

        // populate the neighbor table with mandatory fields (Table 3-63)
        // and optional discovery-time fields (Table 3-64)
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

        // build network descriptors for the confirm primitive
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
    // up would scatter the 3.6.1.4.1.1 bookkeeping across functions
    #[allow(clippy::too_many_lines)]
    pub async fn join(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm {
        if request.rejoin_network == RejoinNetwork::NwkRejoin {
            return self.nwk_rejoin(request).await;
        }
        let fail = Self::join_failure;

        // validate the request (3.2.2.13.3); orphan (0x01) and channel
        // change (0x03) are not yet implemented
        if request.rejoin_network != RejoinNetwork::Association {
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // a device already joined must not re-associate (3.6.1.4.1.1)
        if *self.nib().network_address() != 0xffff {
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // parent selection (3.6.1.4.1.1): reset nwkParentInformation before
        // searching, per spec
        self.reset_parent_negotiation();

        let join_as_router = request.capability_information.device_type();

        let candidates = self.select_parent_candidates(request.extended_pan_id, join_as_router);

        if candidates.is_empty() {
            log::warn!("[NWK-JOIN] no suitable neighbors");
            return fail(NlmeJoinStatus::NotPermitted);
        }

        log::debug!("[NWK-JOIN] neighbor candidates: {candidates:?}");

        // build MAC CapabilityInformation from NWK CapabilityInformation
        // bitmap (Table 3-62)
        let mac_caps = Self::build_mac_capabilities(&request.capability_information);

        // 3.6.1.4.1.1: the capability information shall be stored as the
        // value of the nwkCapabilityInformation NIB attribute
        self.nib()
            .update_capability_information(|value| *value = request.capability_information);

        // try each candidate in order (3.6.1.4.1.1)
        let mut last_status = NlmeJoinStatus::NotPermitted;

        for &candidate_idx in &candidates {
            let table = self.nib().neighbor_table();
            let neighbor = &table[candidate_idx];
            let channel = neighbor.logical_channel;
            let pan_id = PanId(neighbor.pan_id);
            let dest = Address::Short(pan_id, MacShortAddress(neighbor.network_address.0));
            drop(table);

            match self.mac.associate(channel, dest, mac_caps).await {
                Ok(response) => {
                    match response.status {
                        AssociationStatus::Successful => {
                            let assigned_addr = response.association_address;
                            self.nib()
                                .update_network_address(|value| *value = assigned_addr.0);
                            self.nib()
                                .update_ieee_address(|value| *value = response.device_address);
                            self.nib()
                                .update_extended_panid(|value| *value = request.extended_pan_id.0);
                            self.nib().update_panid(|value| *value = pan_id.0);

                            // read parent fields before the clearing loop
                            // zeroes them (3.6.1.4.1.1)
                            let parent_update_id =
                                self.nib().neighbor_table()[candidate_idx].update_id;
                            let parent_channel =
                                self.nib().neighbor_table()[candidate_idx].logical_channel;
                            self.nib()
                                .update_update_id(|value| *value = parent_update_id);

                            // set the relationship field to parent and clear
                            // optional Table 3-64 fields on all entries, which
                            // should not be retained after joining
                            // TODO: retain only entries belonging to the joined
                            // network
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
                            // mark this neighbor as not a potential parent so
                            // we don't retry (3.6.1.4.1.1)
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
                        // other status codes are treated as a generic MAC-level failure
                        _ => {
                            last_status = NlmeJoinStatus::MacError;
                        }
                    }
                }
                Err(_mac_err) => {
                    last_status = NlmeJoinStatus::MacError;
                }
            }
        }

        // all candidates exhausted
        fail(last_status)
    }

    /// NLME-JOIN.request with `RejoinNetwork::NwkRejoin` (3.6.1.4.2).
    ///
    /// Reconnects to a network this device remembers — extended PAN id,
    /// network key — by unicasting a Rejoin Request (3.4.6) and waiting for
    /// the Rejoin Response (3.4.7). The remembered parent is tried first; if
    /// it does not answer, `request.scan_channels` are scanned for another
    /// router of the same network to rejoin through, which may operate on a
    /// different channel.
    ///
    /// `request.security_enabled` selects the rejoin flavour: `true` secures
    /// the Rejoin Request with the active network key (Secure Rejoin,
    /// 4.6.3.3.1), `false` sends it unsecured (Trust Center Rejoin,
    /// 4.6.3.3.2) for a device that no longer holds the key — the caller is
    /// then responsible for obtaining the current network key from the Trust
    /// Center.
    async fn nwk_rejoin(&self, request: NlmeJoinRequest) -> NlmeJoinConfirm {
        let fail = Self::join_failure;

        // a rejoin only makes sense against a network this device already
        // remembers (BDB 7.1 step 4 only reaches here when
        // bdbNodeIsOnANetwork is TRUE and the extended PAN id is known). A
        // leave zeroes nwkExtendedPANId (3.6.1.10.1) without ending the rejoin
        // path, so an unknown extended PAN id takes the requested one
        let remembered_epid = *self.nib().extended_panid();
        if *self.nib().network_address() == 0xffff
            || (remembered_epid != 0 && remembered_epid != request.extended_pan_id.0)
        {
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // whether joining or rejoining, nwkParentInformation is reset
        // (3.2.2.13.3)
        self.reset_parent_negotiation();
        self.nib()
            .update_capability_information(|value| *value = request.capability_information);

        // the remembered parent needs no scan: try it before the discovery
        // below rebuilds the neighbor table from fresh beacons
        let mut last_status = NlmeJoinStatus::InvalidRequest;
        if let Ok(parent) = self.parent_short_address() {
            match self.rejoin_via_parent(parent, &request).await {
                // the channel is whatever the radio is already tuned to
                Ok(response) => return self.rejoin_confirm(response, &request, 0).await,
                Err(status) => {
                    log::debug!("[NWK-REJOIN] remembered parent failed ({status:?})");
                    last_status = status;
                }
            }
        }

        if request.scan_channels.is_empty() {
            return fail(last_status);
        }

        log::debug!(
            "[NWK-REJOIN] scanning {:?} for another parent",
            request.scan_channels
        );
        if let Err(e) = self
            .network_discovery(request.scan_channels.clone(), request.scan_duration)
            .await
        {
            log::warn!("[NWK-REJOIN] scan failed: {e:?}");
            return fail(NlmeJoinStatus::MacError);
        }

        let join_as_router = request.capability_information.device_type();
        let candidates = self.select_rejoin_candidates(request.extended_pan_id, join_as_router);
        if candidates.is_empty() {
            log::warn!("[NWK-REJOIN] no parent candidate found");
            return fail(NlmeJoinStatus::NotPermitted);
        }

        for candidate_idx in candidates {
            let Some((parent, channel)) = self.adopt_parent(candidate_idx).await else {
                continue;
            };
            match self.rejoin_via_parent(parent, &request).await {
                Ok(response) => return self.rejoin_confirm(response, &request, channel).await,
                Err(status) => {
                    log::debug!("[NWK-REJOIN] candidate {parent:?} failed ({status:?})");
                    last_status = status;
                }
            }
        }

        fail(last_status)
    }

    /// One Rejoin Request/Response exchange with `parent` (3.4.6, 3.4.7).
    async fn rejoin_via_parent(
        &self,
        parent: ShortAddress,
        request: &NlmeJoinRequest,
    ) -> Result<RejoinResponse, NlmeJoinStatus> {
        let command = Command::RejoinRequest(RejoinRequest {
            // the Capability Information bit layout (Table 3-62) is shared
            // with `nwk::nib::CapabilityInformation`
            capability_information: rejoin_request::CapabilityInformation(
                request.capability_information.0,
            ),
        });

        let mut buf = [0u8; 128];
        let len = self
            .build_nwk_command_frame(parent, command, request.security_enabled, &mut buf)
            .map_err(|_| NlmeJoinStatus::MacError)?;
        let parent_addr = self
            .parent_address()
            .map_err(|_| NlmeJoinStatus::InvalidRequest)?;

        // arm before transmitting: the response is delivered by the receive
        // path, which may be this procedure's own poll or a concurrent loop
        self.rejoin_response.reset();
        self.mac
            .transmit_data(parent_addr, &buf[..len])
            .await
            .map_err(|_| NlmeJoinStatus::MacError)?;

        let response = self
            .await_rejoin_response(REJOIN_RESPONSE_POLL_RETRIES)
            .await
            .ok_or(NlmeJoinStatus::MacError)?;

        match response.status {
            status if status == u8::from(AssociationStatus::Successful) => Ok(response),
            status if status == u8::from(AssociationStatus::NetworkAtCapacity) => {
                Err(NlmeJoinStatus::PanAtCapacity)
            }
            status if status == u8::from(AssociationStatus::AccessDenied) => {
                Err(NlmeJoinStatus::PanAccessDenied)
            }
            _ => Err(NlmeJoinStatus::MacError),
        }
    }

    /// Build the successful NLME-JOIN.confirm of a rejoin (3.2.2.15).
    async fn rejoin_confirm(
        &self,
        response: RejoinResponse,
        request: &NlmeJoinRequest,
        channel: u8,
    ) -> NlmeJoinConfirm {
        // the receive path stored the assigned address; the hardware filter has
        // to follow it
        self.mac
            .configure(MacConfig::short_address(response.network_address))
            .await;
        // 3.2.2.13.3: nwkExtendedPANId names the network we are on again
        self.nib()
            .update_extended_panid(|value| *value = request.extended_pan_id.0);

        NlmeJoinConfirm {
            status: NlmeJoinStatus::Success,
            network_address: response.network_address,
            extended_pan_id: request.extended_pan_id,
            channel,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        }
    }

    /// Make the neighbor at `candidate_idx` this device's parent and tune the
    /// radio to its channel, returning its address and channel.
    ///
    /// A rejoin may land on a router other than the remembered parent, and the
    /// network may meanwhile have moved to another channel (3.6.1.4.2).
    async fn adopt_parent(&self, candidate_idx: usize) -> Option<(ShortAddress, u8)> {
        let nib = self.nib();
        let (parent, channel, pan_id, update_id) = {
            let table = nib.neighbor_table();
            let neighbor = table.get(candidate_idx)?;
            (
                neighbor.network_address,
                neighbor.logical_channel,
                neighbor.pan_id,
                neighbor.update_id,
            )
        };

        nib.update_neighbor_table(|table| {
            for (index, neighbor) in table.iter_mut().enumerate() {
                neighbor.relationship = if index == candidate_idx {
                    relationship::PARENT
                } else {
                    relationship::NONE
                };
            }
        });
        nib.update_panid(|value| *value = pan_id);
        nib.update_update_id(|value| *value = update_id);
        self.mac.configure(MacConfig::channel(channel)).await;

        Some((parent, channel))
    }

    /// Wait for the receive path to deliver the Rejoin Response (3.4.7).
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
    /// return the APS payload (3.6.2).
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
                // traffic the pre-key joiner cannot decode (SecurityError/ParseError)
                // the NWK-unsecured transport-key (4.6.3.7.2) stays buffered for a
                // later poll
                Ok(None) | Err(NetworkError::SecurityError(_) | NetworkError::ParseError) => (),
                Err(e) => return Err(e),
            }
        }

        Err(NetworkError::MacError(MacError::NoData))
    }

    /// Broadcast an NWK data frame (3.6.5).
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
        // 3.6.5: a sleepy end device unicasts broadcasts to its parent,
        // which relays them into the network on its behalf
        self.transmit_via_parent(&buf[..total_len]).await
    }

    /// Send an NWK data frame to a specific destination (3.6.3).
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
        self.transmit_via_parent(&buf[..total_len]).await
    }
}

#[cfg(test)]
mod tests {
    use core::future::Future;

    use zigbee_mac::AssociationStatus;
    use zigbee_mac::mlme::AssociationResponse;
    use zigbee_mac::mlme::MacConfig;
    use zigbee_mac::mlme::MacError;
    use zigbee_mac::mlme::ScanResult;
    use zigbee_mac::mlme::ScanType;

    use super::*;
    // tests share a global NIB singleton — serialize against every other
    // module that touches it, not just this one
    use crate::TEST_MUTEX;

    // minimal async block_on — the mock futures resolve immediately so a
    // single poll is sufficient
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

    // creates a default `NwkNeighbor` pre-filled for parent selection
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
        // reset() only rewrites fields with a declared default; StorageVec
        // fields (no default) persist across tests in the global NIB,
        // so clear explicitly
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
            scan_channels: 0..0,
            scan_duration: 0,
        }
    }

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

    fn rejoin_request(epid: u64) -> NlmeJoinRequest {
        NlmeJoinRequest {
            extended_pan_id: IeeeAddress(epid),
            rejoin_network: RejoinNetwork::NwkRejoin,
            capability_information: CapabilityInformation(0x80),
            security_enabled: true,
            // no scan: these exercise the remembered-parent path
            scan_channels: 0..0,
            scan_duration: 0,
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
        // 4.6.3.3.2: a Trust Center rejoin carries no NWK security
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
        mac.expect_configure()
            .times(1)
            .withf(|config| config.short_address == Some(ShortAddress(0x5678)))
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

        // 3.6.1.10.1: an end device sends the remove children sub-field as 0
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
        // 3.6.1.10.1: the local leave runs regardless of the confirm status
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

        // 3.6.1.10.3: removed by the parent -> NULL device address
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
        // which 3.6.1.10.1 clears unconditionally for an end device
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

    #[test]
    fn keepalive_method_follows_parent_information() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        let nib = nlme.nib();

        // no negotiated timeout
        assert_eq!(nlme.keepalive_method(), KeepaliveMethod::None);

        nib.update_end_device_timeout(|value| *value = 1);
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE;
        });
        assert_eq!(nlme.keepalive_method(), KeepaliveMethod::TimeoutRequest);

        // bit 0 takes precedence over bit 1
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::MAC_DATA_POLL_KEEPALIVE
                | EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE;
        });
        assert_eq!(nlme.keepalive_method(), KeepaliveMethod::MacDataPoll);
    }

    #[test]
    fn local_timeout_expiry_raises_parent_link_failure() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        seed_joined_nib(&nlme);
        let nib = nlme.nib();

        // no negotiated timeout: nothing to track
        assert!(!nlme.tick_parent_timeout(10_000));
        assert!(nlme.take_nwk_status_indication().is_none());

        // 10 s timeout, refreshed by the negotiation
        nib.update_end_device_timeout(|value| *value = 0);
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::MAC_DATA_POLL_KEEPALIVE;
        });
        nlme.refresh_parent_timeout();

        assert!(!nlme.tick_parent_timeout(4_000));
        assert!(!nlme.tick_parent_timeout(4_000));
        assert!(nlme.tick_parent_timeout(4_000));

        let indication = nlme
            .take_nwk_status_indication()
            .expect("indication raised");
        assert_eq!(indication.status, NetworkStatusCode::ParentLinkFailure);
        assert_eq!(indication.network_address, ShortAddress(0x0000));

        // expiry stops the tracking: it fires once, until the next keepalive
        assert!(!nlme.tick_parent_timeout(4_000));
        assert!(nlme.take_nwk_status_indication().is_none());
    }

    #[test]
    fn keepalive_without_response_raises_parent_link_failure() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));
        mac.expect_poll_data()
            .returning(|_, _| Err(MacError::NoData));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);
        let nib = nlme.nib();
        nib.update_end_device_timeout(|value| *value = 1);
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE;
        });

        let result = block_on(nlme.send_keepalive());
        assert!(matches!(result, Err(NetworkError::ParentLinkFailure)));
        assert_eq!(
            nlme.take_nwk_status_indication().map(|i| i.status),
            Some(NetworkStatusCode::ParentLinkFailure)
        );
    }

    #[test]
    fn keepalive_response_refreshes_local_timeout() {
        // the parent answers the keepalive on the next poll
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));
        mac.expect_poll_data().returning(|_, buf| {
            let frame = encrypted_timeout_response(EndDeviceTimeoutResponse::STATUS_SUCCESS);
            buf[..frame.len()].copy_from_slice(&frame);
            Ok((frame.len(), 200))
        });

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);
        let nib = nlme.nib();
        // request and negotiate the 120 s timeout enumeration
        nib.update_end_device_timeout_default(|value| *value = 1);
        nib.update_end_device_timeout(|value| *value = 1);
        nib.update_parent_information(|value| {
            *value = EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE;
        });

        assert!(block_on(nlme.send_keepalive()).is_ok());
        assert!(nlme.take_nwk_status_indication().is_none());
        // the negotiated 120 s are being tracked again
        assert!(!nlme.tick_parent_timeout(119_000));
        assert!(nlme.tick_parent_timeout(1_000));
    }

    #[test]
    fn repeated_transmit_failures_report_parent_link_failure() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .returning(|_, _| Err(MacError::NoAck));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        // 3.6.3.7: a single failure does not condemn the link
        for _ in 1..MAX_PARENT_TRANSMIT_FAILURES {
            assert!(matches!(
                block_on(nlme.send_data(ShortAddress(0x0000), true, &[1, 2, 3])),
                Err(NetworkError::MacError(_))
            ));
            assert!(nlme.take_nwk_status_indication().is_none());
        }

        // 3.6.3.7.1: the failed link to the parent is reported as 0x09
        assert!(matches!(
            block_on(nlme.send_data(ShortAddress(0x0000), true, &[1, 2, 3])),
            Err(NetworkError::ParentLinkFailure)
        ));
        assert_eq!(
            nlme.take_nwk_status_indication().map(|i| i.status),
            Some(NetworkStatusCode::ParentLinkFailure)
        );
        // the counter restarts, so the next failure is not reported at once
        assert!(matches!(
            block_on(nlme.send_data(ShortAddress(0x0000), true, &[1, 2, 3])),
            Err(NetworkError::MacError(_))
        ));
    }

    #[test]
    fn successful_transmission_clears_the_failure_counter() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .returning(|_, _| Err(MacError::NoAck));
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let _ = block_on(nlme.send_data(ShortAddress(0x0000), true, &[1, 2, 3]));
        assert!(block_on(nlme.send_data(ShortAddress(0x0000), true, &[1, 2, 3])).is_ok());

        let table = nlme.nib().neighbor_table();
        let parent = table
            .iter()
            .find(|n| n.relationship == relationship::PARENT)
            .expect("parent");
        assert_eq!(parent.transmit_failure, 0);
    }

    #[test]
    fn select_rejoin_candidates_ignores_permit_joining() {
        let (_guard, nlme) = make_nlme(MockMlme::new());
        let mut closed = make_neighbor(0xAAAA, 0x1234, 0xDEAD, 200, 1);
        closed.permit_joining = false;
        nlme.nib().update_neighbor_table(|table| {
            let _ = table.push(closed);
        });

        // a closed network still accepts a device it already knows (3.6.1.4.2)
        assert!(
            nlme.select_parent_candidates(IeeeAddress(0xDEAD), false)
                .is_empty()
        );
        assert_eq!(
            nlme.select_rejoin_candidates(IeeeAddress(0xDEAD), false),
            heapless::Vec::<usize, 16>::from_slice(&[0]).unwrap()
        );
    }

    #[test]
    fn rejoin_scans_when_the_known_parent_is_gone() {
        let mut mac = MockMlme::new();
        // the remembered parent no longer answers, so a scan follows
        mac.expect_transmit_data()
            .times(1)
            .returning(|_, _| Err(MacError::NoAck));
        mac.expect_scan_network().times(1).returning(|_, _, _| {
            Ok(ScanResult {
                scan_type: ScanType::Active,
                pan_descriptor: Default::default(),
            })
        });

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let mut request = rejoin_request(0xDEAD);
        request.scan_channels = 20..21;
        let confirm = block_on(nlme.join(request));

        // the scan found no beacon of the remembered network
        assert_eq!(confirm.status, NlmeJoinStatus::NotPermitted);
    }

    #[test]
    fn adopt_parent_follows_the_candidate_channel() {
        let mut mac = MockMlme::new();
        mac.expect_configure()
            .times(1)
            .withf(|config| config.channel == Some(20))
            .returning(|_| ());

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);
        let mut candidate = make_neighbor(0xAAAA, 0x1234, 0xDEAD, 200, 1);
        candidate.logical_channel = 20;
        candidate.update_id = 7;
        nlme.nib().update_neighbor_table(|table| {
            let _ = table.push(candidate);
        });

        let adopted = block_on(nlme.adopt_parent(1));

        assert_eq!(adopted, Some((ShortAddress(0x1234), 20)));
        assert_eq!(nlme.parent_short_address().unwrap(), ShortAddress(0x1234));
        assert_eq!(*nlme.nib().update_id(), 7);
        // the previous parent is demoted: only one entry is the parent
        let table = nlme.nib().neighbor_table();
        assert_eq!(
            table
                .iter()
                .filter(|n| n.relationship == relationship::PARENT)
                .count(),
            1
        );
    }

    #[test]
    fn rejoin_without_scan_channels_keeps_the_remembered_parent_failure() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data()
            .times(1)
            .returning(|_, _| Err(MacError::NoAck));
        // no scan_network expectation: a call here would panic the mock

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::MacError);
    }

    #[test]
    fn leave_request_from_parent_raises_indication_with_rejoin_flag() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);

        let leave = Leave {
            command_options: LeaveCommandOptions(0).set_request(true).set_rejoin(true),
        };
        block_on(nlme.handle_nwk_command(&dummy_header(0x0000), Command::Leave(leave)));

        // 3.2.2.19: removed by the parent -> NULL device address; the higher
        // layer drives the rejoin (3.6.1.10.4 step 1)
        assert_eq!(
            nlme.take_leave_indication(),
            Some(NlmeLeaveIndication {
                device_address: None,
                rejoin: true,
            })
        );
    }

    #[test]
    fn rejoin_after_leave_accepts_the_requested_extended_pan_id() {
        let mut mac = MockMlme::new();
        mac.expect_transmit_data().times(1).returning(|_, _| Ok(()));
        mac.expect_poll_data().returning(|_, buf| {
            let frame = encrypted_rejoin_response(0x5678, 0x00);
            buf[..frame.len()].copy_from_slice(&frame);
            Ok((frame.len(), 200))
        });
        mac.expect_configure().times(1).returning(|_| ());

        let (_guard, nlme) = make_nlme(mac);
        seed_joined_nib(&nlme);
        // a leave zeroed the extended PAN id (3.6.1.10.1)
        nlme.nib().update_extended_panid(|value| *value = 0);

        let confirm = block_on(nlme.join(rejoin_request(0xDEAD)));

        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(*nlme.nib().extended_panid(), 0xDEAD);
    }

    /// An encrypted End Device Timeout Response as the parent sends it.
    fn encrypted_timeout_response(status: u8) -> heapless::Vec<u8, 128> {
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
        let command = Command::EndDeviceTimeoutResponse(EndDeviceTimeoutResponse {
            status,
            parent_information: EndDeviceTimeoutResponse::TIMEOUT_REQUEST_KEEPALIVE,
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
}
