//! Application Support Sub-Layer Management Entity
//!
//! The APSME shall provide a management service to allow an application to
//! interact with the stack.
//!
//! It provides the following services:
//! * Binding management
//! * AIB management
//! * Security
//! * Group management
#![allow(dead_code)]

use core::ops::Not;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use basemgt::ApsmeAddGroupConfirm;
use basemgt::ApsmeAddGroupRequest;
use basemgt::ApsmeBindConfirm;
use basemgt::ApsmeBindRequest;
use basemgt::ApsmeBindRequestStatus;
use basemgt::ApsmeGetConfirm;
use basemgt::ApsmeGetConfirmStatus;
use basemgt::ApsmeRemoveAllGroupsConfirm;
use basemgt::ApsmeRemoveAllGroupsRequest;
use basemgt::ApsmeRemoveGroupConfirm;
use basemgt::ApsmeRemoveGroupRequest;
use basemgt::ApsmeSetConfirm;
use basemgt::ApsmeUnbindConfirm;
use basemgt::ApsmeUnbindRequest;
use basemgt::ApsmeUnbindRequestStatus;
use byte::BytesExt;
use embedded_hal_async::delay::DelayNs;
use spin::Mutex;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::StorageVec;
use zigbee_types::sync::Event;
use zigbee_types::sync::Signal;
use zigbee_types::sync::with_timeout;

use super::aib;
use super::aib::Aib;
use super::aib::DeviceKeyPairDescriptor;
use super::aib::KeyAttribute;
use super::aib::LinkKeyType;
use super::apsde::ApsdeSapIndication;
use super::apsde::ApsdeSapIndicationStatus;
use super::apsde::SecurityStatus;
use super::binding::BindingError;
use super::binding::create_binding_link;
use super::binding::remove_binding_link;
use super::frame::CommandFrame;
use super::frame::Frame;
use super::frame::command::Command;
use super::frame::command::StandardNetworkKeyDescriptor;
use super::frame::command::TransportKey;
use super::frame::command::TrustCenterLinkKeyDescriptor;
use super::frame::frame_control::DeliveryMode;
use super::frame::frame_control::FrameControl;
use super::frame::frame_control::FrameType;
use super::frame::header::Header;
use super::types::Address;
use super::types::DstAddrMode;
use super::types::SrcAddrMode;
use super::types::SrcEndpoint;
use super::types::TxOptions;
use crate::aps::APS_DUPLICATE_REJECTION_TABLE_SIZE;
use crate::aps::APS_MAX_PENDING_ACKS;
use crate::aps::APSC_ACK_WAIT_DURATION_MS;
use crate::aps::APSC_MAX_FRAME_RETRIES;
use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
use crate::nwk::nib::Nib;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::Nlme;
use crate::security::SecurityContext;

pub mod basemgt;
pub mod groupmgt;

// standard network key type of a security material descriptor (3.5.2)
const STANDARD_NETWORK_KEY: u8 = 0x01;

/// Application support sub-layer management service - service access point
///
/// 2.2.4.2
///
/// Supports the transport of management commands between the NHLE and the
/// APSME.
pub trait ApsmeSap {
    /// 2.2.4.3.1 - request to bind two devices together, or to bind a device to
    /// a group
    fn bind_request(&self, request: ApsmeBindRequest) -> ApsmeBindConfirm;
    /// 2.2.4.3.3 - request to unbind two devices, or to unbind a device from a
    /// group
    fn unbind_request(&self, request: ApsmeUnbindRequest) -> ApsmeUnbindConfirm;
    /// 2.2.4.5.1 - APSME-ADD-GROUP.request
    fn add_group(&self, request: ApsmeAddGroupRequest) -> ApsmeAddGroupConfirm;
    /// 2.2.4.5.3 - APSME-REMOVE-GROUP.request
    fn remove_group(&self, request: ApsmeRemoveGroupRequest) -> ApsmeRemoveGroupConfirm;
    /// 2.2.4.5.5 - APSME-REMOVE-ALL-GROUPS.request
    fn remove_all_groups(
        &self,
        request: ApsmeRemoveAllGroupsRequest,
    ) -> ApsmeRemoveAllGroupsConfirm;
}

// identifies the acknowledgement expected for a transmitted frame
// (2.2.8.4.4): same cluster and APS counter, source endpoint equal to the
// destination endpoint the frame was sent to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AckKey {
    destination: u16,
    dst_endpoint: u8,
    cluster_id: u16,
    counter: u8,
}

// one outstanding acknowledged transmission; each slot carries its own
// event because several senders can wait at once and an Event holds a
// single waker
#[derive(Default)]
struct PendingAck {
    key: Mutex<Option<AckKey>>,
    acked: Event,
}

// duplicate rejection table (2.2.8.4.2), keyed on source address and APS
// counter; without a clock the spec's "timing information" degrades to
// insertion-order eviction: an entry is remembered until the ring wraps
#[derive(Default)]
struct DuplicateTable {
    entries: [Option<(u16, u8)>; APS_DUPLICATE_REJECTION_TABLE_SIZE],
    next: usize,
}

impl DuplicateTable {
    // records the frame and reports whether it was seen before
    fn seen(&mut self, source: u16, counter: u8) -> bool {
        if self.entries.contains(&Some((source, counter))) {
            return true;
        }
        self.entries[self.next] = Some((source, counter));
        self.next = (self.next + 1) % APS_DUPLICATE_REJECTION_TABLE_SIZE;
        false
    }
}

// APS Management Entity (2.2.4)
pub(crate) struct Apsme {
    pub(crate) supports_binding_table: bool,
    // whether the device is currently on a network; APS services are only
    // available to a joined device (2.2.8.4.1)
    joined: AtomicBool,
    // transmissions waiting for their acknowledgement (2.2.8.4.4)
    pending_acks: [PendingAck; APS_MAX_PENDING_ACKS],
    // frames already delivered to the next higher layer (2.2.8.4.2)
    duplicates: Mutex<DuplicateTable>,
    // apsCounter AIB attribute (4.4.11)
    pub(crate) aps_counter: AtomicU8,
    // gates handling of Transport-Key/Confirm-Key so replayed or
    // unsolicited frames cannot downgrade an established key (4.4.10)
    tc_exchange_active: AtomicBool,
    // carries whether a transported TC link key was accepted and installed
    // (4.4.10, BDB 10.2.5 step 9)
    pub(crate) tc_key_received: Signal<bool>,
    // carries the Confirm-Key status (4.4.9), 0x00 = success
    pub(crate) tc_key_verified: Signal<u8>,
}

impl Apsme {
    pub(crate) fn new() -> Self {
        Self {
            supports_binding_table: true,
            joined: AtomicBool::new(false),
            pending_acks: Default::default(),
            duplicates: Mutex::new(DuplicateTable::default()),
            aps_counter: AtomicU8::new(0),
            tc_exchange_active: AtomicBool::new(false),
            tc_key_received: Signal::new(),
            tc_key_verified: Signal::new(),
        }
    }

    // arm the TC link key exchange: clear stale events from a previous
    // attempt and accept Transport-Key/Confirm-Key until disarmed
    pub(crate) fn begin_tc_key_exchange(&self) {
        self.tc_key_received.reset();
        self.tc_key_verified.reset();
        self.tc_exchange_active.store(true, Ordering::Release);
    }

    // disarm the TC link key exchange; subsequent key-transport commands
    // are ignored again
    pub(crate) fn end_tc_key_exchange(&self) {
        self.tc_exchange_active.store(false, Ordering::Release);
    }

    // next APS counter value (4.4.11), wrapping
    fn next_aps_counter(&self) -> u8 {
        self.aps_counter
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    /// Track whether the device is on a network, following the ZDO join gate.
    pub(crate) fn set_joined(&self, joined: bool) {
        self.joined.store(joined, Ordering::Release);
    }

    fn is_joined(&self) -> bool {
        self.joined.load(Ordering::Acquire)
    }

    // build and send an APS command frame to a specific destination (4.4);
    // when aps_secure is true the frame is encrypted with the link key for
    // dest_ieee before handing it to the NWK layer, which always applies
    // network-key encryption on top
    pub(crate) async fn send_command<M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        destination: ShortAddress,
        dest_ieee: IeeeAddress,
        command: Command,
        aps_secure: bool,
    ) -> Result<(), NetworkError> {
        let frame_control = FrameControl::default()
            .set_frame_type(FrameType::Command)
            .set_security_flag(aps_secure);

        let header = Header {
            frame_control,
            destination_endpoint: None,
            group_address: None,
            cluster_id: None,
            profile_id: None,
            source_endpoint: None,
            counter: self.next_aps_counter(),
            extended_header: None,
        };

        let mut buf = [0u8; 128];
        let len = if aps_secure {
            let aps_frame = Frame::ApsCommand(CommandFrame { header, command });
            let cx = SecurityContext::get();
            cx.encrypt_aps_frame_in_place(aps_frame, &mut buf, dest_ieee, TxOptions::default())?
        } else {
            let offset = &mut 0;
            buf.write_with(offset, header, ())?;
            buf.write_with(offset, command, ())?;
            *offset
        };

        nlme.send_data(destination, true, &buf[..len]).await
    }

    // poll the parent once (MLME-POLL, 3.6.6) and process the retrieved APS
    // frame; a data frame yields an APSDE-DATA.indication (2.2.4.1.3), a
    // command frame is handled internally (4.4) and yields Ok(None)
    pub(crate) async fn poll_aps_frame<'a, M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        buf: &'a mut [u8],
    ) -> Result<Option<ApsdeSapIndication<'a>>, NetworkError> {
        let Some(nwk_data) = nlme.poll_nwk_frame(buf).await? else {
            return Ok(None);
        };
        self.process_nwk_data(nlme, nwk_data).await
    }

    // passively wait for the next inbound APS frame (rx-on-when-idle
    // devices) and process it like poll_aps_frame
    pub(crate) async fn receive_aps_frame<'a, M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        buf: &'a mut [u8],
    ) -> Result<Option<ApsdeSapIndication<'a>>, NetworkError> {
        let Some(nwk_data) = nlme.receive_nwk_frame(buf).await? else {
            return Ok(None);
        };
        self.process_nwk_data(nlme, nwk_data).await
    }

    // process one inbound NWK data frame into an APSDE-DATA.indication
    // (2.2.4.1.3); APS command frames are handled internally (4.4)
    async fn process_nwk_data<'a, M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        mut nwk_data: crate::nwk::frame::DataFrame<'a>,
    ) -> Result<Option<ApsdeSapIndication<'a>>, NetworkError> {
        let src_short = nwk_data.header.source.0;
        // the NWK destination, not our own address: the higher layer answers a
        // broadcast differently from a unicast (2.4.4.2.7)
        let dst_short = nwk_data.header.destination.0;
        let aps_bytes = nwk_data.payload;

        let offset = &mut 0;
        let header: Header = aps_bytes.read_with(offset, ())?;
        let hdr_len = *offset;
        let secured = header.frame_control.security_flag();

        // acknowledgement of one of our own transmissions (2.2.8.4.4)
        if header.frame_control.frame_type() == FrameType::Acknowledgement {
            self.resolve_ack(AckKey {
                destination: src_short,
                dst_endpoint: header.source_endpoint.unwrap_or(0),
                cluster_id: header.cluster_id.unwrap_or(0),
                counter: header.counter,
            });
            return Ok(None);
        }

        // APS command frame (4.4): process internally, no application data.
        // a broadcast Switch-Key carries no APS security (4.4.6.1.3), so the
        // frame is only decrypted when the security sub-field says so
        if header.frame_control.frame_type() == FrameType::Command {
            let from_trust_center = self.is_trust_center(nlme, aib::get_ref(), src_short);
            let command = if secured {
                // SAFETY: re-borrows the same buffer; decryption is in place
                let aps_buf = unsafe { nwk_data.payload_as_mut() };
                let cx = SecurityContext::get();
                match cx.decrypt_aps_frame_in_place(aps_buf)? {
                    Frame::ApsCommand(CommandFrame { command, .. }) => command,
                    _ => return Ok(None),
                }
            } else {
                let command: Command = aps_bytes[hdr_len..].read_with(&mut 0, ())?;
                // 4.4.1.1 protects every APS command except the broadcast
                // Switch-Key (4.4.6.1.3), so only that one is honored in the
                // clear: any other carries no proof of origin, its addresses
                // being plaintext the sender chose
                if !matches!(command, Command::SwitchKey(_)) {
                    log::warn!("[APS] rx unsecured APS command (ignored)");
                    return Ok(None);
                }
                command
            };
            self.handle_aps_command(aib::get_ref(), nlme.nib(), &command, from_trust_center);
            return Ok(None);
        }

        if header.frame_control.frame_type() != FrameType::Data {
            return Ok(None);
        }

        // an APS-encrypted data frame is decrypted with the link key of its
        // sender before it is dispatched (4.4.1.2)
        let (asdu, security_status) = if secured {
            // SAFETY: re-borrows the same buffer; decryption is in place
            let aps_buf = unsafe { nwk_data.payload_as_mut() };
            let cx = SecurityContext::get();
            match cx.decrypt_aps_frame_in_place(aps_buf)? {
                Frame::Data(data) => (data.payload, SecurityStatus::SecuredLinkKey),
                _ => return Ok(None),
            }
        } else {
            (&aps_bytes[hdr_len..], SecurityStatus::SecuredNwkKey)
        };

        // acknowledge before rejecting a duplicate: a retransmission means our
        // previous acknowledgement was lost (2.2.8.4.2/2.2.8.4.4)
        if header.frame_control.ack_request()
            && let Err(e) = self.send_ack(nlme, ShortAddress(src_short), &header).await
        {
            log::debug!("[APS] tx ack failed: {e:?}");
        }

        if self.duplicates.lock().seen(src_short, header.counter) {
            log::debug!(
                "[APS] duplicate frame from {src_short:#06x} counter {}",
                header.counter
            );
            return Ok(None);
        }

        let src_endpoint =
            SrcEndpoint::new(header.source_endpoint.unwrap_or(0)).unwrap_or_default();

        Ok(Some(ApsdeSapIndication {
            dst_addr_mode: DstAddrMode::Network,
            dst_address: Address::Network(dst_short),
            dst_endpoint: header.destination_endpoint.unwrap_or(0),
            src_addr_mode: SrcAddrMode::Short,
            src_address: Address::Network(src_short),
            src_endpoint,
            profile_id: header.profile_id.unwrap_or(0),
            cluster_id: header.cluster_id.unwrap_or(0),
            asdu,
            status: ApsdeSapIndicationStatus::Success,
            security_status,
            link_quality: 0,
            rx_time: 0,
        }))
    }

    // whether a frame received from `src` came from the Trust Center; the
    // Switch-Key command carries no source address (4.4.10.5), so it is
    // identified by the NWK source of the frame it arrived in
    fn is_trust_center<M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        aib: &Aib,
        src: u16,
    ) -> bool {
        let tc_ieee = *aib.trust_center_address();
        nlme.short_address_for_ieee(tc_ieee)
            .map_or(src == ShortAddress::COORDINATOR.0, |short| short.0 == src)
    }

    // process an inbound APS command frame at its arrival point (4.4);
    // a TC link key is installed as unverified (4.4.10) and a Confirm-Key
    // marks it verified (4.4.9), each signaling BDB 10.2.5 commissioning.
    // a network key transported by the Trust Center becomes the alternate
    // key and its Switch-Key activates it (4.6.3.4.2)
    fn handle_aps_command(&self, aib: &Aib, nib: &Nib, command: &Command, from_trust_center: bool) {
        let exchange_active = self.tc_exchange_active.load(Ordering::Acquire);
        match command {
            // outside an armed exchange a TC link key must not be honored: a
            // replayed frame would downgrade a verified key and reset its
            // frame counters
            Command::TransportKey(TransportKey::TrustCenterLinkKey(_)) if !exchange_active => {
                log::warn!("[APS] rx unsolicited TC link key (ignored)");
            }
            Command::ConfirmKey(_) if !exchange_active => {
                log::warn!("[APS] rx unsolicited confirm key (ignored)");
            }
            Command::TransportKey(TransportKey::TrustCenterLinkKey(descriptor)) => {
                log::debug!("[APS] rx TC link key");
                let installed = self.install_unverified_tc_link_key(aib, descriptor);
                self.tc_key_received.signal(installed);
            }
            Command::ConfirmKey(confirm) => {
                log::debug!("[APS] rx confirm key: status {:#04x}", confirm.status);
                if confirm.status == 0x00 {
                    self.mark_tc_link_key_verified(aib);
                }
                self.tc_key_verified.signal(confirm.status);
            }
            // key transport (4.4.3): only the Trust Center may rotate the
            // network key (4.6.3.4.2)
            Command::TransportKey(TransportKey::StandardNetworkKey(descriptor)) => {
                if descriptor.source_address != *aib.trust_center_address() {
                    log::warn!("[APS] rx network key from a foreign device (ignored)");
                    return;
                }
                log::info!(
                    "[APS] rx network key seq {} as alternate key",
                    descriptor.sequence_number
                );
                install_alternate_network_key(nib, descriptor);
            }
            Command::SwitchKey(switch) if !from_trust_center => {
                log::warn!(
                    "[APS] rx switch key seq {} from a foreign device (ignored)",
                    switch.sequence_number
                );
            }
            Command::SwitchKey(switch) => switch_network_key(nib, switch.sequence_number),
            Command::TransportKey(_) => log::trace!("[APS] rx transport key (ignored)"),
            // TODO: a Request-Key addressed to us (e.g. app link key) would be
            // answered here
            Command::RequestKey(_) => log::trace!("[APS] rx request key (ignored)"),
            Command::VerifyKey(_) => log::trace!("[APS] rx verify key (ignored)"),
            Command::Reserved(id) => log::trace!("[APS] rx reserved command {id:#04x} (ignored)"),
        }
    }

    // install a freshly transported TC link key as unverified (BDB 10.2.5
    // step 9), reporting whether it was accepted: a key identical to the one
    // already held means the exchange failed
    fn install_unverified_tc_link_key(
        &self,
        aib: &Aib,
        descriptor: &TrustCenterLinkKeyDescriptor,
    ) -> bool {
        let tc_ieee = *aib.trust_center_address();
        if aib
            .device_key_pair_set()
            .iter()
            .any(|k| k.device_address == tc_ieee && k.link_key == descriptor.key)
        {
            log::warn!("[APS] transported TC link key is the one already held");
            return false;
        }

        aib.update_device_key_pair_set(|key_set| {
            if let Some(entry) = key_set.iter_mut().find(|k| k.device_address == tc_ieee) {
                entry.link_key = descriptor.key;
                entry.key_attributes = KeyAttribute::UnverifiedKey;
                entry.outgoing_frame_counter = 0;
                entry.incoming_frame_counter = 0;
                entry.link_key_type = LinkKeyType::UniqueLinkKey;
            } else {
                let _ = key_set.push(DeviceKeyPairDescriptor {
                    device_address: tc_ieee,
                    key_attributes: KeyAttribute::UnverifiedKey,
                    link_key: descriptor.key,
                    outgoing_frame_counter: 0,
                    incoming_frame_counter: 0,
                    link_key_type: LinkKeyType::UniqueLinkKey,
                });
            }
        });
        true
    }

    // mark the TC link key as verified after a successful Confirm-Key (4.4.9)
    fn mark_tc_link_key_verified(&self, aib: &Aib) {
        let tc_ieee = *aib.trust_center_address();
        aib.update_device_key_pair_set(|key_set| {
            if let Some(entry) = key_set.iter_mut().find(|k| k.device_address == tc_ieee) {
                entry.key_attributes = KeyAttribute::VerifiedKey;
            }
        });
    }

    // encode an APS data frame into `buf`, returning its length and the APS
    // counter it was stamped with (2.2.5.1)
    fn build_data_frame(
        &self,
        delivery_mode: DeliveryMode,
        ack_request: bool,
        dst_endpoint: u8,
        cluster_id: u16,
        profile_id: u16,
        src_endpoint: u8,
        payload: &[u8],
        buf: &mut [u8],
    ) -> Result<(usize, u8), NetworkError> {
        let frame_control = FrameControl::default()
            .set_frame_type(FrameType::Data)
            .set_delivery_mode(delivery_mode)
            .set_ack_request(ack_request);

        let counter = self.next_aps_counter();
        let header = Header {
            frame_control,
            destination_endpoint: Some(dst_endpoint),
            group_address: None,
            cluster_id: Some(cluster_id),
            profile_id: Some(profile_id),
            source_endpoint: Some(src_endpoint),
            counter,
            extended_header: None,
        };

        let offset = &mut 0;
        buf.write_with(offset, header, ())?;

        let hdr_len = *offset;
        if payload.len() > buf.len() - hdr_len {
            return Err(NetworkError::FrameTooLong);
        }
        buf[hdr_len..hdr_len + payload.len()].copy_from_slice(payload);

        Ok((hdr_len + payload.len(), counter))
    }

    // send a unicast APS data frame to a specific destination (2.2.5.1),
    // without requesting an acknowledgement
    pub(crate) async fn unicast_data<M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        destination: ShortAddress,
        dst_endpoint: u8,
        cluster_id: u16,
        profile_id: u16,
        src_endpoint: u8,
        payload: &[u8],
    ) -> Result<(), NetworkError> {
        let mut buf = [0u8; 100];
        let (len, _) = self.build_data_frame(
            DeliveryMode::Unicast,
            false,
            dst_endpoint,
            cluster_id,
            profile_id,
            src_endpoint,
            payload,
            &mut buf,
        )?;

        nlme.send_data(destination, true, &buf[..len]).await
    }

    /// Send a unicast APS data frame and wait for its acknowledgement
    /// (2.2.8.4.4), retransmitting up to `apscMaxFrameRetries` times.
    ///
    /// The frame is encoded once, so every attempt carries the same APS
    /// counter — which is what both the acknowledgement matching rule and the
    /// receiver's duplicate rejection rely on.
    pub(crate) async fn unicast_data_acked<M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        destination: ShortAddress,
        dst_endpoint: u8,
        cluster_id: u16,
        profile_id: u16,
        src_endpoint: u8,
        payload: &[u8],
        delay: &mut impl DelayNs,
    ) -> Result<(), NetworkError> {
        let mut buf = [0u8; 100];
        let (len, counter) = self.build_data_frame(
            DeliveryMode::Unicast,
            true,
            dst_endpoint,
            cluster_id,
            profile_id,
            src_endpoint,
            payload,
            &mut buf,
        )?;

        let key = AckKey {
            destination: destination.0,
            dst_endpoint,
            cluster_id,
            counter,
        };
        let Some(slot) = self.claim_ack_slot(key) else {
            log::warn!("[APS] no free pending-ack slot");
            return Err(NetworkError::AckTimeout);
        };

        let mut result = Err(NetworkError::AckTimeout);
        for attempt in 0..=APSC_MAX_FRAME_RETRIES {
            if let Err(e) = nlme.send_data(destination, true, &buf[..len]).await {
                result = Err(e);
                break;
            }
            if with_timeout(
                self.pending_acks[slot].acked.wait(),
                delay.delay_ms(APSC_ACK_WAIT_DURATION_MS),
            )
            .await
            .is_some()
            {
                result = Ok(());
                break;
            }
            log::debug!("[APS] ack timeout for counter {counter} (attempt {attempt})");
        }

        self.release_ack_slot(slot);
        result
    }

    /// Reserve a slot for an outstanding acknowledged transmission.
    fn claim_ack_slot(&self, key: AckKey) -> Option<usize> {
        self.pending_acks.iter().position(|pending| {
            let mut slot = pending.key.lock();
            if slot.is_some() {
                return false;
            }
            pending.acked.reset();
            *slot = Some(key);
            true
        })
    }

    fn release_ack_slot(&self, slot: usize) {
        *self.pending_acks[slot].key.lock() = None;
        self.pending_acks[slot].acked.reset();
    }

    /// Release the sender waiting for this acknowledgement, if any.
    fn resolve_ack(&self, key: AckKey) {
        for pending in &self.pending_acks {
            if *pending.key.lock() == Some(key) {
                pending.acked.signal();
                return;
            }
        }
        log::trace!("[APS] rx unexpected ack for counter {}", key.counter);
    }

    /// Acknowledge a received frame that requested it (2.2.5.2.3).
    ///
    /// The acknowledgement echoes the cluster, profile and APS counter of the
    /// original frame and swaps its endpoints. It carries NWK security only —
    /// TODO: mirror the APS security of the frame being acknowledged.
    async fn send_ack<M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        destination: ShortAddress,
        header: &Header,
    ) -> Result<(), NetworkError> {
        let ack = Header {
            frame_control: FrameControl::default()
                .set_frame_type(FrameType::Acknowledgement)
                .set_delivery_mode(DeliveryMode::Unicast),
            destination_endpoint: header.source_endpoint,
            group_address: None,
            cluster_id: header.cluster_id,
            profile_id: header.profile_id,
            source_endpoint: header.destination_endpoint,
            counter: header.counter,
            extended_header: None,
        };

        let mut buf = [0u8; 16];
        let offset = &mut 0;
        buf.write_with(offset, ack, ())?;
        nlme.send_data(destination, true, &buf[..*offset]).await
    }

    // broadcast an APS data frame (2.2.5.1); nwk_broadcast is the NWK
    // broadcast address (e.g. 0xFFFD for RxOnWhenIdle devices)
    pub(crate) async fn broadcast_data<M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        nwk_broadcast: ShortAddress,
        dst_endpoint: u8,
        cluster_id: u16,
        profile_id: u16,
        src_endpoint: u8,
        payload: &[u8],
    ) -> Result<(), NetworkError> {
        let mut buf = [0u8; 100];
        // a broadcast is never acknowledged (2.2.8.4.3)
        let (len, _) = self.build_data_frame(
            DeliveryMode::Broadcast,
            false,
            dst_endpoint,
            cluster_id,
            profile_id,
            src_endpoint,
            payload,
            &mut buf,
        )?;

        nlme.broadcast_data(nwk_broadcast, true, &buf[..len]).await
    }
}

// store a network key transported by the Trust Center as the alternate key,
// leaving the active one in place until a Switch-Key arrives (4.6.3.4.2); all
// frame counters of the replaced key start over
fn install_alternate_network_key(nib: &Nib, descriptor: &StandardNetworkKeyDescriptor) {
    let active = *nib.active_key_seq_number();
    let material = NetworkSecurityMaterialDescriptor {
        key_seq_number: descriptor.sequence_number,
        outgoing_frame_counter: 0,
        incoming_frame_counter_set: StorageVec::new(),
        key: descriptor.key,
        network_key_type: STANDARD_NETWORK_KEY,
    };

    nib.update_security_material_set(|set| {
        // a re-transported key replaces its own entry, a new one the alternate
        let slot = set
            .iter()
            .position(|m| m.key_seq_number == descriptor.sequence_number)
            .or_else(|| set.iter().position(|m| m.key_seq_number != active));
        match slot {
            Some(index) => set[index] = material,
            None => {
                if set.push(material).is_err() {
                    log::warn!("[APS] no room for the alternate network key");
                }
            }
        }
    });

    // a device without material for its active key (e.g. after a rejoin that
    // followed a leave) could not encrypt at all — the transported key becomes
    // the active one instead
    if !nib
        .security_material_set()
        .iter()
        .any(|m| m.key_seq_number == active)
    {
        switch_network_key(nib, descriptor.sequence_number);
    }
}

// activate the network key with the given sequence number (4.6.3.4.2); an
// unknown sequence number is kept as is so the device stays reachable
fn switch_network_key(nib: &Nib, key_seq_number: u8) {
    if !nib
        .security_material_set()
        .iter()
        .any(|m| m.key_seq_number == key_seq_number)
    {
        log::warn!("[APS] rx switch key for unknown network key seq {key_seq_number}");
        return;
    }
    log::info!("[APS] switching to network key seq {key_seq_number}");
    nib.update_active_key_seq_number(|value| *value = key_seq_number);
}

impl ApsmeSap for Apsme {
    // 2.2.4.3.1 - APSME-BIND.request
    // request to bind two devices together, or to bind a device to a group
    fn bind_request(&self, request: ApsmeBindRequest) -> ApsmeBindConfirm {
        let status = if !self.is_joined() || !self.supports_binding_table {
            ApsmeBindRequestStatus::IllegalRequest
        } else if let Some(binding) = request.binding() {
            let mut result = Ok(());
            aib::get_ref()
                .update_binding_table(|table| result = create_binding_link(table, binding));
            match result {
                Ok(()) => ApsmeBindRequestStatus::Success,
                Err(BindingError::TableFull) => ApsmeBindRequestStatus::TableFull,
                Err(_) => ApsmeBindRequestStatus::IllegalRequest,
            }
        } else {
            ApsmeBindRequestStatus::IllegalRequest
        };

        ApsmeBindConfirm {
            status,
            src_address: request.src_address,
            src_endpoint: request.src_endpoint,
            cluster_id: request.cluster_id,
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
        }
    }

    // 2.2.4.3.3 - request to unbind two devices, or to unbind a device from a
    // group
    fn unbind_request(&self, request: ApsmeUnbindRequest) -> ApsmeUnbindConfirm {
        let status = if self.is_joined().not() {
            ApsmeUnbindRequestStatus::IllegalRequest
        } else if let Some(binding) = request.binding() {
            let mut result = Ok(());
            aib::get_ref()
                .update_binding_table(|table| result = remove_binding_link(table, &binding));
            match result {
                Ok(()) => ApsmeUnbindRequestStatus::Success,
                Err(BindingError::InvalidBinding) => ApsmeUnbindRequestStatus::InvalidBinding,
                Err(_) => ApsmeUnbindRequestStatus::IllegalRequest,
            }
        } else {
            ApsmeUnbindRequestStatus::IllegalRequest
        };

        ApsmeUnbindConfirm {
            status,
            src_address: request.src_address,
            src_endpoint: request.src_endpoint,
            cluster_id: request.cluster_id,
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
        }
    }

    fn add_group(&self, _request: ApsmeAddGroupRequest) -> ApsmeAddGroupConfirm {
        ApsmeAddGroupConfirm {}
    }

    fn remove_group(&self, _request: ApsmeRemoveGroupRequest) -> ApsmeRemoveGroupConfirm {
        todo!()
    }

    fn remove_all_groups(
        &self,
        _request: ApsmeRemoveAllGroupsRequest,
    ) -> ApsmeRemoveAllGroupsConfirm {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use basemgt::ApsmeBindRequestStatus;
    use zigbee_types::ByteArray;

    use super::*;
    use crate::aps::aib::MAX_APS_BINDING_TABLE;
    use crate::aps::binding::BindingAddrMode;
    use crate::aps::frame::command::ConfirmKey;
    use crate::aps::frame::command::SwitchKey;
    use crate::aps::types::SrcEndpoint;

    fn bind_request(cluster_id: u16) -> ApsmeBindRequest {
        ApsmeBindRequest {
            src_address: Address::Extended(0xaabb_ccdd_eeff_0011),
            src_endpoint: SrcEndpoint::new(10).unwrap(),
            cluster_id,
            dst_addr_mode: BindingAddrMode::Device,
            dst_address: Address::Extended(0x1122_3344_5566_7788),
            dst_endpoint: 2,
        }
    }

    fn unbind_request(cluster_id: u16) -> ApsmeUnbindRequest {
        let request = bind_request(cluster_id);
        ApsmeUnbindRequest {
            src_address: request.src_address,
            src_endpoint: request.src_endpoint,
            cluster_id: request.cluster_id,
            dst_addr_mode: request.dst_addr_mode,
            dst_address: request.dst_address,
            dst_endpoint: request.dst_endpoint,
        }
    }

    // 2.2.4.3.1/2.2.4.3.3 — the binding table lives in the AIB singleton, so
    // the whole lifecycle runs as one test rather than racing sibling tests
    #[test]
    fn bind_request_lifecycle() {
        aib::try_init();
        aib::get_ref().update_binding_table(|table| table.clear());

        let mut apsme = Apsme::new();

        // a device without binding support rejects the request
        apsme.supports_binding_table = false;
        apsme.set_joined(true);
        assert_eq!(
            apsme.bind_request(bind_request(1)).status,
            ApsmeBindRequestStatus::IllegalRequest
        );

        // so does a device that has not joined a network
        apsme.supports_binding_table = true;
        apsme.set_joined(false);
        assert_eq!(
            apsme.bind_request(bind_request(1)).status,
            ApsmeBindRequestStatus::IllegalRequest
        );

        apsme.set_joined(true);
        assert_eq!(
            apsme.bind_request(bind_request(1)).status,
            ApsmeBindRequestStatus::Success
        );
        // an identical binding is not stored twice
        assert_eq!(
            apsme.bind_request(bind_request(1)).status,
            ApsmeBindRequestStatus::Success
        );
        assert_eq!(aib::get_ref().binding_table().len(), 1);

        for cluster in 2..=MAX_APS_BINDING_TABLE as u16 {
            assert_eq!(
                apsme.bind_request(bind_request(cluster)).status,
                ApsmeBindRequestStatus::Success
            );
        }
        assert_eq!(
            apsme
                .bind_request(bind_request(MAX_APS_BINDING_TABLE as u16 + 1))
                .status,
            ApsmeBindRequestStatus::TableFull
        );

        assert_eq!(
            apsme.unbind_request(unbind_request(1)).status,
            ApsmeUnbindRequestStatus::Success
        );
        assert_eq!(
            apsme.unbind_request(unbind_request(1)).status,
            ApsmeUnbindRequestStatus::InvalidBinding
        );
    }

    // 2.2.8.4.4: an ack matches on cluster, APS counter and a source endpoint
    // equal to the destination endpoint the frame was sent to
    #[test]
    fn ack_matching_releases_only_the_waiting_sender() {
        let apsme = Apsme::new();
        let key = AckKey {
            destination: 0x0000,
            dst_endpoint: 1,
            cluster_id: 0x0402,
            counter: 42,
        };
        let slot = apsme.claim_ack_slot(key).unwrap();

        // wrong counter, wrong endpoint and wrong cluster all miss
        apsme.resolve_ack(AckKey { counter: 41, ..key });
        apsme.resolve_ack(AckKey {
            dst_endpoint: 2,
            ..key
        });
        apsme.resolve_ack(AckKey {
            cluster_id: 0x0006,
            ..key
        });
        assert!(!apsme.pending_acks[slot].acked.is_set());

        apsme.resolve_ack(key);
        assert!(apsme.pending_acks[slot].acked.is_set());

        // a released slot no longer matches and can be claimed again
        apsme.release_ack_slot(slot);
        apsme.resolve_ack(key);
        assert!(!apsme.pending_acks[slot].acked.is_set());
        assert!(apsme.claim_ack_slot(key).is_some());
    }

    #[test]
    fn duplicate_table_rejects_repeats_until_evicted() {
        let mut table = DuplicateTable::default();

        assert!(!table.seen(0x1234, 7));
        assert!(table.seen(0x1234, 7));
        // same counter from another device is a different frame
        assert!(!table.seen(0x4321, 7));

        // the ring forgets the oldest entry once it wraps
        for counter in 0..APS_DUPLICATE_REJECTION_TABLE_SIZE as u8 {
            assert!(!table.seen(0x1234, 100 + counter));
        }
        assert!(!table.seen(0x1234, 7));
    }

    const TC_IEEE: IeeeAddress = IeeeAddress(0xaaaa_bbbb_cccc_dddd);
    const NEW_KEY: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ];

    fn setup_aib() -> Aib {
        let aib = Aib::new();
        aib.update_trust_center_address(|value| *value = TC_IEEE);
        aib
    }

    fn tc_link_key() -> Command {
        Command::TransportKey(TransportKey::TrustCenterLinkKey(
            TrustCenterLinkKeyDescriptor {
                key: ByteArray(NEW_KEY),
                destination_address: IeeeAddress(1),
                source_address: TC_IEEE,
            },
        ))
    }

    fn confirm_key(status: u8) -> Command {
        Command::ConfirmKey(ConfirmKey {
            status,
            key_type: 0x04,
            destination_address: IeeeAddress(1),
        })
    }

    // the NIB is only touched by the network-key commands, so the link-key
    // cases hand in a throwaway one
    fn handle(apsme: &Apsme, aib: &Aib, command: &Command) {
        apsme.handle_aps_command(aib, &Nib::new(), command, true);
    }

    #[test]
    fn tc_link_key_is_installed_unverified_and_signaled() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::Context;
        use core::task::Waker;

        let apsme = Apsme::new();
        let aib = setup_aib();
        let mut cx = Context::from_waker(Waker::noop());
        apsme.begin_tc_key_exchange();

        let mut received = pin!(apsme.tc_key_received.wait());
        assert!(received.as_mut().poll(&mut cx).is_pending());

        handle(&apsme, &aib, &tc_link_key());

        let entry = aib.device_key_pair_set()[0].clone();
        assert_eq!(entry.device_address, TC_IEEE);
        assert_eq!(entry.link_key, ByteArray(NEW_KEY));
        assert_eq!(entry.key_attributes, KeyAttribute::UnverifiedKey);
        assert_eq!(
            received.as_mut().poll(&mut cx),
            core::task::Poll::Ready(true)
        );
    }

    // BDB 10.2.5 step 9: a transported key identical to the one already held
    // fails the exchange instead of resetting the frame counters
    #[test]
    fn transported_tc_link_key_identical_to_the_held_one_is_rejected() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::Context;
        use core::task::Poll;
        use core::task::Waker;

        let apsme = Apsme::new();
        let aib = setup_aib();
        let mut cx = Context::from_waker(Waker::noop());
        apsme.begin_tc_key_exchange();

        handle(&apsme, &aib, &tc_link_key());
        aib.update_device_key_pair_set(|key_set| key_set[0].outgoing_frame_counter = 42);

        handle(&apsme, &aib, &tc_link_key());
        assert_eq!(
            pin!(apsme.tc_key_received.wait()).poll(&mut cx),
            Poll::Ready(false)
        );
        assert_eq!(aib.device_key_pair_set()[0].outgoing_frame_counter, 42);
    }

    #[test]
    fn confirm_key_marks_verified_and_signals_status() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::Context;
        use core::task::Poll;
        use core::task::Waker;

        let apsme = Apsme::new();
        let aib = setup_aib();
        let mut cx = Context::from_waker(Waker::noop());
        apsme.begin_tc_key_exchange();
        handle(&apsme, &aib, &tc_link_key());

        // failed verification signals the status but leaves the key unverified
        handle(&apsme, &aib, &confirm_key(0x01));
        assert_eq!(
            aib.device_key_pair_set()[0].key_attributes,
            KeyAttribute::UnverifiedKey
        );
        assert_eq!(
            pin!(apsme.tc_key_verified.wait()).poll(&mut cx),
            Poll::Ready(0x01)
        );

        handle(&apsme, &aib, &confirm_key(0x00));
        assert_eq!(
            aib.device_key_pair_set()[0].key_attributes,
            KeyAttribute::VerifiedKey
        );
        assert_eq!(
            pin!(apsme.tc_key_verified.wait()).poll(&mut cx),
            Poll::Ready(0x00)
        );
    }

    #[test]
    fn key_transport_ignored_outside_exchange() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::Context;
        use core::task::Waker;

        let apsme = Apsme::new();
        let aib = setup_aib();
        let mut cx = Context::from_waker(Waker::noop());

        // not armed: a (replayed) TC link key must not touch the AIB
        handle(&apsme, &aib, &tc_link_key());
        assert!(aib.device_key_pair_set().is_empty());
        assert!(
            pin!(apsme.tc_key_received.wait())
                .poll(&mut cx)
                .is_pending()
        );

        handle(&apsme, &aib, &confirm_key(0x00));
        assert!(
            pin!(apsme.tc_key_verified.wait())
                .poll(&mut cx)
                .is_pending()
        );

        // disarmed after a completed exchange: a replay must not downgrade
        // the verified key or reset its frame counters
        apsme.begin_tc_key_exchange();
        handle(&apsme, &aib, &tc_link_key());
        handle(&apsme, &aib, &confirm_key(0x00));
        apsme.end_tc_key_exchange();
        aib.update_device_key_pair_set(|key_set| key_set[0].outgoing_frame_counter = 42);

        handle(&apsme, &aib, &tc_link_key());
        let entry = aib.device_key_pair_set()[0].clone();
        assert_eq!(entry.key_attributes, KeyAttribute::VerifiedKey);
        assert_eq!(entry.outgoing_frame_counter, 42);
    }

    fn network_key(sequence_number: u8, key: [u8; 16], source: IeeeAddress) -> Command {
        Command::TransportKey(TransportKey::StandardNetworkKey(
            StandardNetworkKeyDescriptor {
                key: ByteArray(key),
                sequence_number,
                destination_address: IeeeAddress(1),
                source_address: source,
            },
        ))
    }

    // 4.6.3.4.2: a transported network key becomes the alternate key and only
    // the Switch-Key from the Trust Center makes it active
    // seeds the material set with an active network key of sequence number 3
    fn nib_with_active_key() -> Nib {
        let nib = Nib::new();
        nib.update_security_material_set(|set| {
            let _ = set.push(NetworkSecurityMaterialDescriptor {
                key_seq_number: 3,
                outgoing_frame_counter: 7,
                incoming_frame_counter_set: StorageVec::new(),
                key: ByteArray([0xaa; 16]),
                network_key_type: STANDARD_NETWORK_KEY,
            });
        });
        nib.update_active_key_seq_number(|value| *value = 3);
        nib
    }

    #[test]
    fn network_key_update_replaces_the_alternate_key_and_switches_on_command() {
        let apsme = Apsme::new();
        let aib = setup_aib();
        let nib = nib_with_active_key();

        apsme.handle_aps_command(&aib, &nib, &network_key(4, NEW_KEY, TC_IEEE), true);

        // the active key is untouched, the new one is stored alongside it
        assert_eq!(*nib.active_key_seq_number(), 3);
        let set = nib.security_material_set();
        assert_eq!(set.len(), 2);
        assert_eq!(set[1].key_seq_number, 4);
        assert_eq!(set[1].outgoing_frame_counter, 0);
        assert_eq!(set[0].outgoing_frame_counter, 7);
        drop(set);

        apsme.handle_aps_command(
            &aib,
            &nib,
            &Command::SwitchKey(SwitchKey { sequence_number: 4 }),
            true,
        );
        assert_eq!(*nib.active_key_seq_number(), 4);

        // the next rotation overwrites the now-alternate key, not the active one
        apsme.handle_aps_command(&aib, &nib, &network_key(5, [0xbb; 16], TC_IEEE), true);
        let set = nib.security_material_set();
        assert_eq!(set.len(), 2);
        assert_eq!(set[0].key_seq_number, 5);
        assert_eq!(set[1].key_seq_number, 4);
    }

    #[test]
    fn network_key_and_switch_key_from_a_foreign_device_are_ignored() {
        let apsme = Apsme::new();
        let aib = setup_aib();
        let nib = nib_with_active_key();

        // a key transported by anyone but the Trust Center
        apsme.handle_aps_command(
            &aib,
            &nib,
            &network_key(4, NEW_KEY, IeeeAddress(0x99)),
            true,
        );
        assert_eq!(nib.security_material_set().len(), 1);

        apsme.handle_aps_command(&aib, &nib, &network_key(4, NEW_KEY, TC_IEEE), true);
        // a switch from a device that is not the Trust Center
        apsme.handle_aps_command(
            &aib,
            &nib,
            &Command::SwitchKey(SwitchKey { sequence_number: 4 }),
            false,
        );
        assert_eq!(*nib.active_key_seq_number(), 3);

        // as is a switch to a key we never received
        apsme.handle_aps_command(
            &aib,
            &nib,
            &Command::SwitchKey(SwitchKey { sequence_number: 9 }),
            true,
        );
        assert_eq!(*nib.active_key_seq_number(), 3);
    }
}

#[cfg(test)]
mod receive_path_tests {
    use core::future::Future;

    use zigbee_mac::Address;
    use zigbee_mac::mlme::AssociationResponse;
    use zigbee_mac::mlme::MacConfig;
    use zigbee_mac::mlme::MacError;
    use zigbee_mac::mlme::Mlme;
    use zigbee_mac::mlme::ScanResult;
    use zigbee_mac::mlme::ScanType;
    use zigbee_types::ByteArray;
    use zigbee_types::IeeeAddress;
    use zigbee_types::ShortAddress;

    use super::*;
    // NIB and AIB are process-wide singletons
    use crate::TEST_MUTEX;
    use crate::nwk::frame::DataFrame;
    use crate::nwk::frame::frame_control::DiscoverRoute;
    use crate::nwk::frame::frame_control::FrameControl as NwkFrameControl;
    use crate::nwk::frame::frame_control::FrameType as NwkFrameType;
    use crate::nwk::frame::header::Header as NwkHeader;
    use crate::nwk::nib;

    const TC_IEEE: IeeeAddress = IeeeAddress(0x00_11_22_33_44_55_66_77);
    const TC_SHORT: u16 = 0x0000;
    const ACTIVE_KEY_SEQ: u8 = 3;
    const ACTIVE_KEY: [u8; 16] = [0xaa; 16];
    const ATTACKER_KEY: [u8; 16] = [0xbb; 16];

    // never transmits, exists so an Nlme can be built
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

    // the receive path never yields
    #[allow(clippy::panic)]
    fn block_on<F: Future>(f: F) -> F::Output {
        use core::pin::pin;
        use core::task::Context;
        use core::task::Poll;
        use core::task::Waker;

        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut f = pin!(f);
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(val) => val,
            Poll::Pending => panic!("block_on: future returned Pending"),
        }
    }

    fn setup() -> Nlme<MockMlme> {
        nib::try_init();
        nib::reset();
        aib::try_init();
        aib::get_ref().update_trust_center_address(|value| *value = TC_IEEE);
        // an empty neighbor table makes is_trust_center fall back to the
        // coordinator, which TC_SHORT satisfies
        nib::get_ref().update_neighbor_table(|value| *value = StorageVec::new());
        nib::get_ref().update_security_material_set(|set| {
            *set = StorageVec::new();
            let _ = set.push(NetworkSecurityMaterialDescriptor {
                key_seq_number: ACTIVE_KEY_SEQ,
                outgoing_frame_counter: 7,
                incoming_frame_counter_set: StorageVec::new(),
                key: ByteArray(ACTIVE_KEY),
                network_key_type: STANDARD_NETWORK_KEY,
            });
        });
        nib::get_ref().update_active_key_seq_number(|value| *value = ACTIVE_KEY_SEQ);

        let mut mac = MockMlme::new();
        mac.expect_ieee_address()
            .return_const(IeeeAddress(0xa4c1_0000_0000_0001));
        Nlme::new(mac)
    }

    // an inbound NWK data frame as the NWK layer hands it to the APS. a shared
    // slice suffices while every frame here is APS-unsecured; only the secured
    // branch reaches for payload_as_mut, which casts the payload to &mut
    fn nwk_data_frame(aps: &[u8]) -> DataFrame<'_> {
        let header = NwkHeader {
            frame_control: NwkFrameControl(0)
                .set_frame_type(NwkFrameType::Data)
                .set_protocol_version(2)
                .set_discover_route(DiscoverRoute::Suppress),
            destination: ShortAddress(0x1234),
            source: ShortAddress(TC_SHORT),
            radius: 1,
            sequence_number: 0,
            destination_ieee: None,
            source_ieee: None,
            multicast_control: None,
            source_route_subframe: None,
        };
        DataFrame {
            header,
            payload: aps,
        }
    }

    // frame control 0x01 (command, unicast, security clear), APS counter, then
    // the payload. command frames carry no endpoint/cluster/profile octets
    fn unsecured_aps_command(counter: u8, payload: &[u8]) -> std::vec::Vec<u8> {
        let mut frame = std::vec![0x01u8, counter];
        frame.extend_from_slice(payload);
        frame
    }

    // 4.4.1.1: command id, key type, key, sequence number, destination IEEE,
    // source IEEE
    fn transport_network_key_payload(
        key: [u8; 16],
        seq: u8,
        source: IeeeAddress,
    ) -> std::vec::Vec<u8> {
        let mut payload = std::vec![0x05u8, 0x01u8];
        payload.extend_from_slice(&key);
        payload.push(seq);
        payload.extend_from_slice(&IeeeAddress(0x0102_0304_0506_0708).0.to_le_bytes());
        payload.extend_from_slice(&source.0.to_le_bytes());
        payload
    }

    // 4.4.6.1.3 exempts only the broadcast Switch-Key from APS security, so an
    // unsecured network-key transport proves nothing: its source address is
    // plaintext the sender chose
    #[test]
    fn unsecured_network_key_transport_is_ignored() {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let nlme = setup();
        let apsme = Apsme::new();

        // names the real Trust Center as the source
        let payload = transport_network_key_payload(ATTACKER_KEY, 4, TC_IEEE);
        let frame = unsecured_aps_command(0x42, &payload);
        let indication = block_on(apsme.process_nwk_data(&nlme, nwk_data_frame(&frame)))
            .expect("an APS command frame is consumed, not reported as an error");
        assert!(
            indication.is_none(),
            "a command frame yields no data indication"
        );

        let set = nib::get_ref().security_material_set();
        assert_eq!(
            set.len(),
            1,
            "an APS-unsecured Transport-Key must not install a network key: only \
             the broadcast Switch-Key is exempt from APS security (4.4.6.1.3), and \
             the descriptor's source_address is attacker-controlled plaintext here"
        );
        assert_eq!(set[0].key_seq_number, ACTIVE_KEY_SEQ);
        assert_eq!(set[0].key, ByteArray(ACTIVE_KEY));
    }

    // closing the hole above must not close the one command that legitimately
    // arrives unsecured (4.4.6.1.3)
    #[test]
    fn unsecured_switch_key_is_still_honoured() {
        let _guard = TEST_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let nlme = setup();
        let apsme = Apsme::new();

        // give the device an alternate key to switch to
        nib::get_ref().update_security_material_set(|set| {
            let _ = set.push(NetworkSecurityMaterialDescriptor {
                key_seq_number: 4,
                outgoing_frame_counter: 0,
                incoming_frame_counter_set: StorageVec::new(),
                key: ByteArray([0xcc; 16]),
                network_key_type: STANDARD_NETWORK_KEY,
            });
        });

        let frame = unsecured_aps_command(0x43, &[0x09, 0x04]);
        let _ = block_on(apsme.process_nwk_data(&nlme, nwk_data_frame(&frame)))
            .expect("an APS command frame is consumed, not reported as an error");

        assert_eq!(
            *nib::get_ref().active_key_seq_number(),
            4,
            "a broadcast Switch-Key carries no APS security by design (4.4.6.1.3)"
        );
    }
}
