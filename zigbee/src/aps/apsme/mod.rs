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
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::sync::Event;

use super::aib;
use super::aib::Aib;
use super::aib::DeviceKeyPairDescriptor;
use super::aib::KeyAttribute;
use super::aib::LinkKeyType;
use super::apsde::ApsdeSapIndication;
use super::apsde::ApsdeSapIndicationStatus;
use super::apsde::SecurityStatus;
use super::binding::ApsBindingTable;
use super::frame::CommandFrame;
use super::frame::Frame;
use super::frame::command::Command;
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
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::Nlme;
use crate::security::SecurityContext;

pub mod basemgt;
pub mod groupmgt;

/// Application support sub-layer management service - service access point
///
/// 2.2.4.2
///
/// Supports the transport of management commands between the NHLE and the
/// APSME.
pub trait ApsmeSap {
    /// 2.2.4.3.1 - request to bind two devices together, or to bind a device to
    /// a group
    fn bind_request(&mut self, request: ApsmeBindRequest) -> ApsmeBindConfirm;
    /// 2.2.4.3.3 - request to unbind two devices, or to unbind a device from a
    /// group
    fn unbind_request(&mut self, request: ApsmeUnbindRequest) -> ApsmeUnbindConfirm;
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

/// APS Management Entity (§2.2.4).
pub(crate) struct Apsme {
    pub(crate) supports_binding_table: bool,
    pub(crate) binding_table: ApsBindingTable,
    pub(crate) joined_network: Option<Address>,
    /// apsCounter AIB attribute (§4.4.11)
    pub(crate) aps_counter: AtomicU8,
    /// whether a TC link key exchange is in flight; gates the handling of
    /// Transport-Key/Confirm-Key so replayed or unsolicited frames cannot
    /// downgrade an established key (§4.4.10)
    tc_exchange_active: AtomicBool,
    /// signaled when a TC link key was received and installed (§4.4.10)
    pub(crate) tc_key_received: Event,
    /// signaled when the TC answered the key verification (§4.4.9)
    pub(crate) tc_key_verified: Event,
    /// status of the last Confirm-Key (§4.4.9), valid once
    /// [`Self::tc_key_verified`] fired
    tc_confirm_status: AtomicU8,
}

impl Apsme {
    pub(crate) fn new() -> Self {
        Self {
            supports_binding_table: true,
            binding_table: ApsBindingTable::new(),
            joined_network: None,
            aps_counter: AtomicU8::new(0),
            tc_exchange_active: AtomicBool::new(false),
            tc_key_received: Event::new(),
            tc_key_verified: Event::new(),
            tc_confirm_status: AtomicU8::new(0),
        }
    }

    /// Arm the TC link key exchange: clear stale events from a previous
    /// attempt and accept Transport-Key/Confirm-Key until disarmed.
    pub(crate) fn begin_tc_key_exchange(&self) {
        self.tc_key_received.reset();
        self.tc_key_verified.reset();
        self.tc_exchange_active.store(true, Ordering::Release);
    }

    /// Disarm the TC link key exchange; subsequent key-transport commands
    /// are ignored again.
    pub(crate) fn end_tc_key_exchange(&self) {
        self.tc_exchange_active.store(false, Ordering::Release);
    }

    /// Status of the last Confirm-Key (0x00 = success), valid after
    /// [`Self::tc_key_verified`] fired.
    pub(crate) fn tc_confirm_status(&self) -> u8 {
        self.tc_confirm_status.load(Ordering::Acquire)
    }

    /// Next APS counter value (§4.4.11), wrapping.
    fn next_aps_counter(&self) -> u8 {
        self.aps_counter
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    fn is_joined(&self) -> bool {
        self.joined_network.is_some()
    }

    /// Build and send an APS command frame to a specific destination (§4.4).
    ///
    /// When `aps_secure` is true the APS frame is encrypted with the link key
    /// for `dest_ieee` before handing it to the NWK layer. The NWK layer
    /// always encrypts with the network key.
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

    /// Poll the parent once (MLME-POLL, §3.6.6) and process the retrieved APS
    /// frame.
    ///
    /// An APS **data** frame is surfaced as an APSDE-DATA.indication
    /// (§2.2.4.1.3); an APS **command** frame is processed internally (§4.4)
    /// and yields `Ok(None)`. ZDO/ZCL traffic from a centralized
    /// coordinator is NWK-encrypted only, so only APS-unsecured data frames
    /// produce an indication. `src_address` carries the NWK source so the
    /// caller can address a response back to the requester.
    pub(crate) async fn poll_aps_frame<'a, M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        buf: &'a mut [u8],
    ) -> Result<Option<ApsdeSapIndication<'a>>, NetworkError> {
        let Some(nwk_data) = nlme.poll_nwk_frame(buf).await? else {
            return Ok(None);
        };
        self.process_nwk_data(nlme, nwk_data)
    }

    /// Passively wait for the next inbound APS frame (rx-on-when-idle
    /// devices) and process it like [`Self::poll_aps_frame`].
    pub(crate) async fn receive_aps_frame<'a, M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        buf: &'a mut [u8],
    ) -> Result<Option<ApsdeSapIndication<'a>>, NetworkError> {
        let Some(nwk_data) = nlme.receive_nwk_frame(buf).await? else {
            return Ok(None);
        };
        self.process_nwk_data(nlme, nwk_data)
    }

    /// Process one inbound NWK data frame into an APSDE-DATA.indication
    /// (§2.2.4.1.3); APS command frames are handled internally (§4.4).
    fn process_nwk_data<'a, M: zigbee_mac::mlme::Mlme>(
        &self,
        nlme: &Nlme<M>,
        mut nwk_data: crate::nwk::frame::DataFrame<'a>,
    ) -> Result<Option<ApsdeSapIndication<'a>>, NetworkError> {
        let src_short = nwk_data.header.source.0;
        let local_addr = *nlme.nib().network_address();
        let aps_bytes = nwk_data.payload;

        let offset = &mut 0;
        let header: Header = aps_bytes.read_with(offset, ())?;

        // APS command frame (§4.4): process internally, no application data.
        if header.frame_control.frame_type() == FrameType::Command {
            // SAFETY: re-borrows the same buffer; decryption is in place
            let aps_buf = unsafe { nwk_data.payload_as_mut() };
            let cx = SecurityContext::get();
            let frame = cx.decrypt_aps_frame_in_place(aps_buf)?;
            if let Frame::ApsCommand(CommandFrame { command, .. }) = frame {
                self.handle_aps_command(aib::get_ref(), &command);
            }
            return Ok(None);
        }

        // only APS-unsecured data frames are dispatched as indications
        if header.frame_control.frame_type() != FrameType::Data
            || header.frame_control.security_flag()
        {
            return Ok(None);
        }

        let asdu = &aps_bytes[*offset..];
        let src_endpoint =
            SrcEndpoint::new(header.source_endpoint.unwrap_or(0)).unwrap_or_default();

        Ok(Some(ApsdeSapIndication {
            dst_addr_mode: DstAddrMode::Network,
            dst_address: Address::Network(local_addr),
            dst_endpoint: header.destination_endpoint.unwrap_or(0),
            src_addr_mode: SrcAddrMode::Short,
            src_address: Address::Network(src_short),
            src_endpoint,
            profile_id: header.profile_id.unwrap_or(0),
            cluster_id: header.cluster_id.unwrap_or(0),
            asdu,
            status: ApsdeSapIndicationStatus::Success,
            security_status: SecurityStatus::SecuredNwkKey,
            link_quality: 0,
            rx_time: 0,
        }))
    }

    /// Process an inbound APS command frame at its arrival point (§4.4).
    ///
    /// Security-manager duties are performed inline: a Trust Center link key
    /// is installed as unverified (§4.4.10) and a Confirm-Key marks it
    /// verified (§4.4.9); each signals the corresponding event so a
    /// commissioning flow (BDB §10.2.5) can await progress. Unsolicited NWK
    /// key rotation (§4.4.3) is a future extension point.
    fn handle_aps_command(&self, aib: &Aib, command: &Command) {
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
                self.install_unverified_tc_link_key(aib, descriptor);
                self.tc_key_received.signal();
            }
            Command::ConfirmKey(confirm) => {
                log::debug!("[APS] rx confirm key: status {:#04x}", confirm.status);
                if confirm.status == 0x00 {
                    self.mark_tc_link_key_verified(aib);
                }
                self.tc_confirm_status
                    .store(confirm.status, Ordering::Release);
                self.tc_key_verified.signal();
            }
            // key transport (§4.4.3): the network key is obtained inline during
            // join; an unsolicited key update would be handled here
            Command::TransportKey(_) => log::trace!("[APS] rx transport key (ignored)"),
            // TODO: a Request-Key addressed to us (e.g. app link key) would be
            // answered here
            Command::RequestKey(_) => log::trace!("[APS] rx request key (ignored)"),
            Command::VerifyKey(_) => log::trace!("[APS] rx verify key (ignored)"),
            Command::Reserved(id) => log::trace!("[APS] rx reserved command {id:#04x} (ignored)"),
        }
    }

    /// Install a freshly transported TC link key as unverified (§4.4.10 step 9
    /// of BDB §10.2.5).
    fn install_unverified_tc_link_key(&self, aib: &Aib, descriptor: &TrustCenterLinkKeyDescriptor) {
        let tc_ieee = *aib.trust_center_address();
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
    }

    /// Mark the TC link key as verified after a successful Confirm-Key
    /// (§4.4.9).
    fn mark_tc_link_key_verified(&self, aib: &Aib) {
        let tc_ieee = *aib.trust_center_address();
        aib.update_device_key_pair_set(|key_set| {
            if let Some(entry) = key_set.iter_mut().find(|k| k.device_address == tc_ieee) {
                entry.key_attributes = KeyAttribute::VerifiedKey;
            }
        });
    }

    /// Send a unicast APS data frame to a specific destination (§2.2.5.1).
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
        let frame_control = FrameControl::default()
            .set_frame_type(FrameType::Data)
            .set_delivery_mode(DeliveryMode::Unicast);

        let header = Header {
            frame_control,
            destination_endpoint: Some(dst_endpoint),
            group_address: None,
            cluster_id: Some(cluster_id),
            profile_id: Some(profile_id),
            source_endpoint: Some(src_endpoint),
            counter: self.next_aps_counter(),
            extended_header: None,
        };

        let mut buf = [0u8; 100];
        let offset = &mut 0;
        buf.write_with(offset, header, ())?;

        let hdr_len = *offset;
        let payload_len = payload.len().min(buf.len() - hdr_len);
        buf[hdr_len..hdr_len + payload_len].copy_from_slice(&payload[..payload_len]);

        nlme.send_data(destination, true, &buf[..hdr_len + payload_len])
            .await
    }

    /// Broadcast an APS data frame (§2.2.5.1).
    ///
    /// `nwk_broadcast` is the NWK broadcast address (e.g. `0xFFFD` for
    /// RxOnWhenIdle devices).
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
        let frame_control = FrameControl::default()
            .set_frame_type(FrameType::Data)
            .set_delivery_mode(DeliveryMode::Broadcast);

        let header = Header {
            frame_control,
            destination_endpoint: Some(dst_endpoint),
            group_address: None,
            cluster_id: Some(cluster_id),
            profile_id: Some(profile_id),
            source_endpoint: Some(src_endpoint),
            counter: self.next_aps_counter(),
            extended_header: None,
        };

        let mut buf = [0u8; 100];
        let offset = &mut 0;
        buf.write_with(offset, header, ())?;

        let hdr_len = *offset;
        let payload_len = payload.len().min(buf.len() - hdr_len);
        buf[hdr_len..hdr_len + payload_len].copy_from_slice(&payload[..payload_len]);

        nlme.broadcast_data(nwk_broadcast, true, &buf[..hdr_len + payload_len])
            .await
    }
}

impl ApsmeSap for Apsme {
    /// 2.2.4.3.1 - APSME-BIND.request
    /// request to bind two devices together, or to bind a device to a group
    fn bind_request(&mut self, request: ApsmeBindRequest) -> ApsmeBindConfirm {
        let status = if !self.is_joined() || !self.supports_binding_table {
            ApsmeBindRequestStatus::IllegalRequest
        } else if self.binding_table.is_full() {
            ApsmeBindRequestStatus::TableFull
        } else {
            match self.binding_table.create_binding_link(&request) {
                Ok(_) => ApsmeBindRequestStatus::Success,
                Err(_) => ApsmeBindRequestStatus::IllegalRequest,
            }
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

    /// 2.2.4.3.3 - request to unbind two devices, or to unbind a device from a
    /// group
    fn unbind_request(&mut self, request: ApsmeUnbindRequest) -> ApsmeUnbindConfirm {
        let status = if self.is_joined().not() {
            ApsmeUnbindRequestStatus::IllegalRequest
        } else {
            let res = self.binding_table.remove_binding_link(&request);
            match res {
                Ok(_) => ApsmeUnbindRequestStatus::Success,
                Err(err) => match err {
                    crate::aps::binding::BindingError::IllegalRequest
                    | crate::aps::binding::BindingError::TableFull => {
                        ApsmeUnbindRequestStatus::IllegalRequest
                    }
                    crate::aps::binding::BindingError::InvalidBinding => {
                        ApsmeUnbindRequestStatus::InvalidBinding
                    }
                },
            }
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

    /// 2.2.4.5.1 - APSME-ADD-GROUP.request
    fn add_group(&self, _request: ApsmeAddGroupRequest) -> ApsmeAddGroupConfirm {
        ApsmeAddGroupConfirm {}
    }

    /// 2.2.4.5.3 - APSME-REMOVE-GROUP.request
    fn remove_group(&self, _request: ApsmeRemoveGroupRequest) -> ApsmeRemoveGroupConfirm {
        todo!()
    }

    /// 2.2.4.5.5 - APSME-REMOVE-ALL-GROUPS.request
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
    use crate::aps::frame::command::ConfirmKey;
    use crate::aps::types::SrcEndpoint;

    // 2.2.4.3.1
    #[test]
    fn bind_request_device_does_not_support_binding_should_fail() {
        // given
        let mut apsme = Apsme::new();
        apsme.supports_binding_table = false;
        let request = ApsmeBindRequest {
            src_address: Address::Extended(0u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };

        // when
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::IllegalRequest);
    }

    // 2.2.4.3.1
    #[test]
    fn bind_request_from_an_unjoined_device_should_fail() {
        // given
        let mut apsme = Apsme::new();
        let request = ApsmeBindRequest {
            src_address: Address::Extended(0u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };

        // when
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::IllegalRequest);
    }

    // 2.2.4.3.1
    #[test]
    fn bind_request_with_full_table_should_fail() {
        // given
        let mut apsme = Apsme::new();
        apsme.joined_network = Some(Address::Extended(10u64));
        for n in 0..265u64 {
            let request = ApsmeBindRequest {
                src_address: Address::Extended(n),
                src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
                cluster_id: 1u16,
                dst_addr_mode: 0u8,
                dst_address: 1u8,
                dst_endpoint: 2u8,
            };
            let _ = apsme.bind_request(request);
        }

        // when
        let request = ApsmeBindRequest {
            src_address: Address::Extended(999u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::TableFull);
    }

    #[test]
    fn bind_request_with_valid_request_should_succeed() {
        // given
        let mut apsme = Apsme::new();
        apsme.joined_network = Some(Address::Extended(10u64));

        // when
        let request = ApsmeBindRequest {
            src_address: Address::Extended(999u64),
            src_endpoint: SrcEndpoint::new(10).unwrap_or(SrcEndpoint { value: 0 }),
            cluster_id: 1u16,
            dst_addr_mode: 0u8,
            dst_address: 1u8,
            dst_endpoint: 2u8,
        };
        let result = apsme.bind_request(request);

        // then
        assert_eq!(result.status, ApsmeBindRequestStatus::Success);
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

        apsme.handle_aps_command(&aib, &tc_link_key());

        let entry = aib.device_key_pair_set()[0].clone();
        assert_eq!(entry.device_address, TC_IEEE);
        assert_eq!(entry.link_key, ByteArray(NEW_KEY));
        assert_eq!(entry.key_attributes, KeyAttribute::UnverifiedKey);
        assert!(received.as_mut().poll(&mut cx).is_ready());
    }

    #[test]
    fn confirm_key_marks_verified_and_signals_status() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::Context;
        use core::task::Waker;

        let apsme = Apsme::new();
        let aib = setup_aib();
        let mut cx = Context::from_waker(Waker::noop());
        apsme.begin_tc_key_exchange();
        apsme.handle_aps_command(&aib, &tc_link_key());

        // failed verification signals the status but leaves the key unverified
        apsme.handle_aps_command(&aib, &confirm_key(0x01));
        assert_eq!(
            aib.device_key_pair_set()[0].key_attributes,
            KeyAttribute::UnverifiedKey
        );
        assert!(pin!(apsme.tc_key_verified.wait()).poll(&mut cx).is_ready());
        assert_eq!(apsme.tc_confirm_status(), 0x01);

        apsme.handle_aps_command(&aib, &confirm_key(0x00));
        assert_eq!(
            aib.device_key_pair_set()[0].key_attributes,
            KeyAttribute::VerifiedKey
        );
        assert!(pin!(apsme.tc_key_verified.wait()).poll(&mut cx).is_ready());
        assert_eq!(apsme.tc_confirm_status(), 0x00);
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
        apsme.handle_aps_command(&aib, &tc_link_key());
        assert!(aib.device_key_pair_set().is_empty());
        assert!(
            pin!(apsme.tc_key_received.wait())
                .poll(&mut cx)
                .is_pending()
        );

        apsme.handle_aps_command(&aib, &confirm_key(0x00));
        assert!(
            pin!(apsme.tc_key_verified.wait())
                .poll(&mut cx)
                .is_pending()
        );

        // disarmed after a completed exchange: a replay must not downgrade
        // the verified key or reset its frame counters
        apsme.begin_tc_key_exchange();
        apsme.handle_aps_command(&aib, &tc_link_key());
        apsme.handle_aps_command(&aib, &confirm_key(0x00));
        apsme.end_tc_key_exchange();
        aib.update_device_key_pair_set(|key_set| key_set[0].outgoing_frame_counter = 42);

        apsme.handle_aps_command(&aib, &tc_link_key());
        let entry = aib.device_key_pair_set()[0].clone();
        assert_eq!(entry.key_attributes, KeyAttribute::VerifiedKey);
        assert_eq!(entry.outgoing_frame_counter, 42);
    }
}
