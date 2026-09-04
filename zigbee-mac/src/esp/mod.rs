use alloc::vec::Vec;

use byte::BytesExt;
use embassy_futures::select::Either;
use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Timer;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;
use ieee802154::mac::Address;
use ieee802154::mac::FrameContent;
use ieee802154::mac::FrameType;
use ieee802154::mac::FrameVersion;
use ieee802154::mac::Header;
use ieee802154::mac::PanId;
use ieee802154::mac::command::CapabilityInformation;
use ieee802154::mac::command::Command;
use ieee802154::mac::security::SecurityContext;

use crate::esp::driver::Ieee802154Driver;
use crate::esp::fair_mutex::FairMutex;
use crate::esp::fair_mutex::FairMutexGuard;
use crate::mlme::A_BASE_SUPER_FRAME_DURATION;
use crate::mlme::A_MAX_FRAME_RETRIES;
use crate::mlme::A_RESPONSE_WAIT_TIME;
use crate::mlme::AssociationResponse;
use crate::mlme::MAX_IEEE802154_CHANNELS;
use crate::mlme::MacConfig;
use crate::mlme::MacError;
use crate::mlme::Mlme;
use crate::mlme::PanDescriptor;
use crate::mlme::PanDescriptorList;
use crate::mlme::ScanResult;
use crate::mlme::ScanType;

mod driver;
mod fair_mutex;

// higher-layer retries of the whole association handshake; aMaxFrameRetries
// ack-retransmission already covers a lost request or poll, this is a safety
// net for a parent that accepts the request but is slow to respond
const ASSOCIATE_REQUEST_RETRIES: u8 = 3;

// number of times the association response is polled per request attempt
const ASSOCIATE_POLL_RETRIES: u8 = 5;

// number of poll rounds per steady-state MLME-POLL before reporting no data
const POLL_DATA_RETRIES: u8 = 5;

const RADIO_LOCK_TIMEOUT_MS: u64 = 5_000;

/// `esp-radio` [`Mlme`] implementation
///
/// The radio is a single shared resource: the inner state is held behind an
/// async mutex so the trait's `&self` methods can be driven concurrently from a
/// receive task and a transmit path. The extended address is cached so it can
/// be read without locking.
pub struct EspMlme<'a> {
    inner: FairMutex<CriticalSectionRawMutex, EspMlmeInner<'a>>,
    ieee_address: u64,
}

impl<'a> EspMlme<'a> {
    pub fn new(ieee802154: Ieee802154<'a>, config: Config) -> Self {
        // seed the MAC seq number randomly (7.2.1.2): a fixed start re-uses low
        // numbers each reboot, which a parent's duplicate filter drops as stale
        let inner = EspMlmeInner {
            driver: Ieee802154Driver::new(ieee802154, config),
            seq_number: (esp_hal::rng::Rng::new().random() & 0xff) as u8,
        };
        let ieee_address = inner.driver.ieee_address().0;
        Self {
            inner: FairMutex::new(inner),
            ieee_address,
        }
    }

    /// The device's IEEE 802.15.4 extended (EUI-64) address.
    pub fn ieee_address(&self) -> u64 {
        self.ieee_address
    }

    async fn lock(
        &self,
    ) -> Result<FairMutexGuard<'_, CriticalSectionRawMutex, EspMlmeInner<'a>>, MacError> {
        match select(
            Timer::after_millis(RADIO_LOCK_TIMEOUT_MS),
            self.inner.lock(),
        )
        .await
        {
            Either::First(_) => Err(MacError::RadioLockTimeout),
            Either::Second(Ok(guard)) => Ok(guard),
            Either::Second(Err(_)) => {
                log::warn!("[MAC] radio lock queue full");
                Err(MacError::RadioLockQueueFull)
            }
        }
    }

    async fn receive_data(&self, buf: &mut [u8]) -> Result<(usize, u8), MacError> {
        loop {
            // drain under a brief lock, then idle-wait lock-free so a
            // concurrent transmit can acquire the radio while we
            // wait for the next frame
            {
                let mut inner = self.lock().await?;
                if let Some(received) = inner.try_drain(buf)? {
                    return Ok(received);
                }
            }
            driver::wait_rx_signal().await;
        }
    }
}

struct EspMlmeInner<'a> {
    driver: Ieee802154Driver<'a>,
    seq_number: u8,
}

impl EspMlmeInner<'_> {
    fn sequence_number(&mut self) -> u8 {
        self.seq_number = self.seq_number.wrapping_add(1);
        self.seq_number
    }

    // retransmits up to aMaxFrameRetries times if unacknowledged (IEEE 802.15.4
    // 7.5.6.4); returns MacError::NoAck if every attempt goes unacknowledged
    async fn transmit_acked(&mut self, frame: &[u8]) -> Result<(), MacError> {
        for _ in 0..=A_MAX_FRAME_RETRIES {
            match self.driver.transmit(frame).await {
                Ok(()) if self.driver.last_tx_acked() => return Ok(()),
                Ok(()) => {}
                // no ack / channel busy: retransmit (7.5.6.4)
                Err(MacError::TxFailed) => {}
                // a lost radio interrupt: retry rather than wedge the caller
                Err(MacError::TxTimeout) => log::warn!("[MLME] tx-done timeout, retrying"),
                Err(e) => return Err(e),
            }
        }
        Err(MacError::NoAck)
    }

    fn flush(&mut self) {
        while self.driver.poll_received().is_some() {}
    }

    fn poll_frame(&mut self) -> Option<Result<ReceivedFrame, MacError>> {
        self.driver
            .poll_received()
            .map(|r| r.map_err(MacError::RadioError))
    }

    async fn next_frame(&mut self) -> Result<ReceivedFrame, MacError> {
        loop {
            // wait for a fully-received frame before draining: draining
            // re-issues RxStart and aborts a frame still on the air
            // (dropped a fast ~2ms indirect response). signal is
            // level-held, so drain until empty then reset
            self.driver.wait_rx_available().await;
            if let Some(result) = self.poll_frame() {
                return result;
            }
            self.driver.reset_rx_signal();
        }
    }

    fn try_drain(&mut self, buf: &mut [u8]) -> Result<Option<(usize, u8)>, MacError> {
        while let Some(result) = self.poll_frame() {
            if let Some(received) = copy_data_payload(result?, buf) {
                return Ok(Some(received));
            }
        }
        Ok(None)
    }

    fn beacon_request_frame(&mut self) -> [u8; 10] {
        let seq_number = self.sequence_number();
        [0x3, 0x8, seq_number, 0xff, 0xff, 0xff, 0xff, 0x7, 0x0, 0x0]
    }

    async fn scan_channel_active(
        &mut self,
        channel: u8,
        duration: u8,
    ) -> Result<Option<PanDescriptorList>, MacError> {
        self.flush();
        self.driver.update_driver_config(|config| {
            config.promiscuous = false;
            config.channel = channel;
        });
        self.driver.start_receive();

        let frame = self.beacon_request_frame();
        if let Err(e) = self.driver.transmit(&frame).await {
            log::error!("[MLME-SCAN]: error transmitting beacon: {e}");
        }

        log::debug!("[MLME-SCAN] sent beacon frame to channel {channel}, waiting for messages...");

        let delay_us: u64 = calculate_scan_duration_max_us(duration).into();
        log::debug!("[MLME-SCAN] waiting for response for {delay_us}us");

        let mut pds = Vec::new();
        let deadline = Timer::after_micros(delay_us);
        let mut deadline = core::pin::pin!(deadline);

        loop {
            match embassy_futures::select::select(&mut deadline, self.next_frame()).await {
                Either::First(_) => break,
                Either::Second(Ok(frame)) => {
                    if let Some(pd) = self.parse_beacon(frame) {
                        pds.push(pd);
                    }
                }
                Either::Second(Err(_)) => continue,
            }
        }

        Ok(Some(pds))
    }

    fn parse_beacon(&self, received: ReceivedFrame) -> Option<PanDescriptor> {
        match received {
            ReceivedFrame {
                frame:
                    Frame {
                        header:
                            hdr @ Header {
                                source: Some(source),
                                ..
                            },
                        content: FrameContent::Beacon(beacon_content),
                        payload,
                        ..
                    },
                channel,
                lqi,
                ..
            } => {
                log::debug!("[MLME-SCAN] received beacon frame on channel {channel}");

                let zigbee_beacon = match payload.read_with(&mut 0, ()) {
                    Ok(zb) => zb,
                    Err(e) => {
                        log::warn!("[MLME-SCAN] failed to parse zigbee beacon: {e:?}");
                        return None;
                    }
                };

                Some(PanDescriptor {
                    channel,
                    coord_addr_mode: match source {
                        Address::Short(_, _) => 0x2,
                        Address::Extended(_, _) => 0x3,
                    },
                    coord_pan_id: source.pan_id().0.into(),
                    coord_address: source,
                    superframe_spec: beacon_content.superframe_spec,
                    link_quality: lqi,
                    security_use: hdr.has_security(),
                    zigbee_beacon,
                })
            }
            other => {
                log::debug!("[MLME-SCAN] received non-beacon frame: {other:?}");
                None
            }
        }
    }

    // MAC data request command frame (IEEE 802.15.4 7.3.4). returns the buffer
    // and on-air length (written bytes + 2 for the FCS the radio appends); the
    // source address mode varies (extended before a short address is assigned,
    // short after) so the length is not fixed — caller must transmit exactly
    // &buf[..len], not the whole fixed-size array
    fn data_request_frame(&mut self, dest: Address) -> Result<([u8; 20], usize), MacError> {
        let seq = self.sequence_number();
        let source = match self.driver.short_address() {
            Some(short) => Address::Short(dest.pan_id(), ieee802154::mac::ShortAddress(short)),
            None => Address::Extended(dest.pan_id(), self.driver.ieee_address()),
        };
        let frame_header = Header {
            frame_type: FrameType::MacCommand,
            frame_pending: false,
            ack_request: true,
            pan_id_compress: true,
            seq_no_suppress: false,
            ie_present: false,
            version: FrameVersion::Ieee802154_2003,
            seq,
            destination: Some(dest),
            source: Some(source),
            auxiliary_security_header: None,
        };
        let frame_content = FrameContent::Command(Command::DataRequest);

        let mut buf = [0u8; 20];
        let offset = &mut 0;
        buf.write_with(
            offset,
            frame_header,
            &Some(&mut SecurityContext::no_security()),
        )?;
        buf.write_with(offset, frame_content, ())?;

        Ok((buf, *offset + 2))
    }

    // listens up to timeout_us for a MAC association response, draining and
    // discarding any other frame. does not flush, so a response already
    // queued (a parent that sent it directly) is not lost
    async fn recv_association_response(
        &mut self,
        timeout_us: u64,
    ) -> Result<Option<AssociationResponse>, MacError> {
        // wait ~4ms before each drain (> a frame's ~0.8ms air time): draining
        // mid-reception re-issues RxStart and aborts the frame. time-based, not
        // signal-based: the RX signal may not fire even when the frame is
        // queued. no logging in the loop — the UART critical section
        // stalls the RX ISR
        let timeout = Timer::after_micros(timeout_us);
        let receive = async {
            loop {
                Timer::after_micros(4000).await;
                while let Some(result) = self.poll_frame() {
                    if let ReceivedFrame {
                        frame:
                            Frame {
                                content:
                                    FrameContent::Command(Command::AssociationResponse(
                                        short_addr,
                                        status,
                                    )),
                                ..
                            },
                        ..
                    } = result?
                    {
                        return Ok::<_, MacError>(AssociationResponse {
                            device_address: zigbee_types::IeeeAddress(self.driver.ieee_address().0),
                            association_address: zigbee_types::ShortAddress(short_addr.0),
                            status,
                        });
                    }
                }
            }
        };
        match embassy_futures::select::select(timeout, receive).await {
            Either::First(_) => Ok(None),
            Either::Second(Ok(r)) => Ok(Some(r)),
            Either::Second(Err(e)) => Err(e),
        }
    }

    // an indirect (poll) response is always a unicast, so ambient broadcasts
    // never shadow it
    fn take_unicast_data(&mut self, buf: &mut [u8]) -> Result<Option<(usize, u8)>, MacError> {
        while let Some(result) = self.poll_frame() {
            let received = result?;
            if matches!(
                received.frame.header.destination,
                Some(Address::Short(_, ieee802154::mac::ShortAddress(d))) if d >= 0xfff8
            ) {
                continue;
            }
            if let Some(data) = copy_data_payload(received, buf) {
                return Ok(Some(data));
            }
        }
        Ok(None)
    }

    // shares recv_association_response's timing discipline: waits ~4ms before
    // each drain (the RX signal may not fire even when a frame is queued) and
    // never logs in the loop (the UART critical section stalls the RX ISR)
    async fn recv_data_response(
        &mut self,
        timeout_us: u64,
        buf: &mut [u8],
    ) -> Result<Option<(usize, u8)>, MacError> {
        let timeout = Timer::after_micros(timeout_us);
        let receive = async {
            loop {
                Timer::after_micros(4000).await;
                if let Some(data) = self.take_unicast_data(buf)? {
                    return Ok::<_, MacError>(Some(data));
                }
            }
        };
        match embassy_futures::select::select(timeout, receive).await {
            Either::First(_) => Ok(None),
            Either::Second(result) => result,
        }
    }

    fn association_request_frame(
        &mut self,
        dest: Address,
        src: Option<Address>,
        capabilities: CapabilityInformation,
    ) -> Result<[u8; 21], MacError> {
        let seq = self.sequence_number();
        let frame_header = Header {
            frame_type: FrameType::MacCommand,
            frame_pending: false,
            ack_request: true,
            pan_id_compress: false,
            seq_no_suppress: false,
            ie_present: false,
            version: FrameVersion::Ieee802154_2003,
            seq,
            destination: Some(dest),
            source: src,
            auxiliary_security_header: None,
        };
        let frame_content = FrameContent::Command(Command::AssociationRequest(capabilities));

        let mut buf = [0u8; 21];
        let offset = &mut 0;
        buf.write_with(
            offset,
            frame_header,
            &Some(&mut SecurityContext::no_security()),
        )?;
        buf.write_with(offset, frame_content, ())?;

        Ok(buf)
    }
}

// non-data frames (commands, beacons, acks) yield None
fn copy_data_payload(received: ReceivedFrame, buf: &mut [u8]) -> Option<(usize, u8)> {
    let ReceivedFrame {
        frame:
            Frame {
                content: FrameContent::Data,
                payload,
                ..
            },
        lqi,
        ..
    } = received
    else {
        return None;
    };
    let len = payload.len().min(buf.len());
    buf[..len].copy_from_slice(&payload[..len]);
    Some((len, lqi))
}

fn calculate_scan_duration_max_us(duration: u8) -> u32 {
    // we assume a symbol period of 16us (QPSK, 2.4Ghz)
    16 * A_BASE_SUPER_FRAME_DURATION * (2 * (duration as u32) + 1)
}

impl EspMlmeInner<'_> {
    async fn scan_network(
        &mut self,
        scan_type: ScanType,
        channels: core::ops::Range<u8>,
        duration: u8,
    ) -> Result<ScanResult, MacError> {
        if !matches!(scan_type, ScanType::Active) {
            return Err(MacError::InvalidScanParams);
        }

        log::debug!("[MLME-SCAN] start scan");

        let mut pan_descriptor = Vec::new();
        for c in channels {
            if (c as usize) >= MAX_IEEE802154_CHANNELS {
                continue;
            }

            match self.scan_channel_active(c, duration).await {
                Ok(Some(mut pd)) => {
                    pan_descriptor.append(&mut pd);
                }
                Err(e) => {
                    log::error!("[MLME-SCAN] error on channel {c}: {e}");
                }
                _ => (),
            }
        }

        log::debug!("[MLME-SCAN] success");

        Ok(ScanResult {
            scan_type,
            pan_descriptor,
        })
    }

    async fn associate(
        &mut self,
        channel: u8,
        dest: Address,
        capabilities: CapabilityInformation,
    ) -> Result<AssociationResponse, MacError> {
        // filter on our extended address: the association response is an
        // indirect tx addressed to it (7.5.6.4). auto_ack_rx must stay
        // on to ack it — promiscuous suppresses the ack and floods the
        // RX queue with broadcasts
        self.driver.update_driver_config(|config| {
            *config = Default::default();
            config.channel = channel;
            config.pan_id = Some(dest.pan_id().0);
            config.auto_ack_tx = true;
            config.auto_ack_rx = true;
            config.promiscuous = false;
        });
        // arm RX after the config change: otherwise general RX is off until a
        // later TX triggers rx-on-when-idle and the response is missed
        self.driver.start_receive();

        let ext_addr = self.driver.ieee_address();
        // 7.3.1.1: source PAN is the broadcast PAN (0xffff), source is the ext
        // addr
        let src = Address::Extended(PanId::broadcast(), ext_addr);
        let timeout_us = (A_RESPONSE_WAIT_TIME as u64) * 16;

        // retry the full handshake (7.5.3.1): a lost/unacked request leaves the
        // parent with nothing to buffer, so re-send each round. within a round
        // listen first (rx-on-when-idle parent replies directly) then poll for
        // indirect delivery. never flush — a direct response may already be
        // queued
        let mut response = None;
        'association: for _ in 0..ASSOCIATE_REQUEST_RETRIES {
            let frame = self.association_request_frame(dest, Some(src), capabilities)?;
            match self.transmit_acked(&frame).await {
                Ok(()) => {}
                Err(MacError::NoAck) => {
                    log::debug!("[MLME-ASSOCIATE] request not acked, retrying");
                    continue;
                }
                Err(e) => return Err(e),
            }
            log::debug!(
                "[MLME-ASSOCIATE] request acked, ack_pending={:?}",
                self.driver.last_ack_frame_pending()
            );

            // catch a directly-sent response before spending a poll round-trip
            if let Some(r) = self.recv_association_response(timeout_us).await? {
                response = Some(r);
                break 'association;
            }

            for _ in 0..ASSOCIATE_POLL_RETRIES {
                let (data_req, len) = self.data_request_frame(dest)?;
                match self.transmit_acked(&data_req[..len]).await {
                    Ok(()) | Err(MacError::NoAck) => {}
                    Err(e) => return Err(e),
                }
                // arm RX for the indirect response (~2-3ms after the poll ack);
                // start_receive is a no-op if already receiving. no log here —
                // the UART critical section stalls the RX ISR
                // and a TI parent replies once
                self.driver.start_receive();
                if let Some(r) = self.recv_association_response(timeout_us).await? {
                    response = Some(r);
                    break 'association;
                }
            }
        }
        let response = response.ok_or(MacError::NoData)?;

        log::debug!(
            "[MLME-ASSOCIATE] success, short_addr={:?}",
            response.association_address
        );

        // set the assigned short address so the hw filter accepts our unicasts
        let short = response.association_address.0;
        self.driver.update_driver_config(|config| {
            config.promiscuous = false;
            config.short_addr = Some(short);
        });

        Ok(response)
    }

    async fn poll_data(
        &mut self,
        coord_address: Address,
        buf: &mut [u8],
    ) -> Result<(usize, u8), MacError> {
        // a response elicited by an earlier poll may have landed after its
        // listen window closed; deliver it instead of flushing it away
        if let Some((len, lqi)) = self.take_unicast_data(buf)? {
            log::trace!("[MLME-POLL] rx buffered data len={len}");
            return Ok((len, lqi));
        }
        let timeout_us = (A_RESPONSE_WAIT_TIME as u64) * 16;

        // retry the poll handshake (7.5.6.3)
        let mut acked = false;
        for _ in 0..POLL_DATA_RETRIES {
            let (data_req, len) = self.data_request_frame(coord_address)?;
            match self.transmit_acked(&data_req[..len]).await {
                // no ack after retries: the parent may be busy; keep listening
                Ok(()) => acked = true,
                Err(MacError::NoAck) => {}
                Err(e) => return Err(e),
            }
            // arm RX for the indirect response (~2-3ms after the poll ack);
            // start_receive is a no-op if already receiving
            self.driver.start_receive();
            if let Some((len, lqi)) = self.recv_data_response(timeout_us, buf).await? {
                log::trace!("[MLME-POLL] rx data len={len}");
                return Ok((len, lqi));
            }
        }
        // an unacknowledged poll says nothing about buffered data: the parent
        // itself is unreachable, which the NWK layer treats as a missed
        // keepalive (3.6.10.3)
        if acked {
            log::trace!("[MLME-POLL] no data");
            Err(MacError::NoData)
        } else {
            log::trace!("[MLME-POLL] no ack");
            Err(MacError::NoAck)
        }
    }

    async fn transmit_data(&mut self, dest: Address, payload: &[u8]) -> Result<(), MacError> {
        let seq = self.sequence_number();

        // NWK broadcast addresses (0xfff8-0xffff) map to the MAC broadcast
        // address 0xffff, which is never acknowledged (IEEE 802.15.4 7.2.1.1.2)
        let is_broadcast = matches!(dest, Address::Short(_, sa) if sa.0 >= 0xfff8);
        let dest = if is_broadcast {
            Address::Short(dest.pan_id(), ieee802154::mac::ShortAddress(0xffff))
        } else {
            dest
        };

        let source = Some(match self.driver.short_address() {
            Some(short) => Address::Short(dest.pan_id(), ieee802154::mac::ShortAddress(short)),
            None => Address::Extended(dest.pan_id(), self.driver.ieee_address()),
        });

        let frame_header = Header {
            frame_type: FrameType::Data,
            frame_pending: false,
            ack_request: !is_broadcast,
            pan_id_compress: source.is_some(),
            seq_no_suppress: false,
            ie_present: false,
            version: FrameVersion::Ieee802154_2003,
            seq,
            destination: Some(dest),
            source,
            auxiliary_security_header: None,
        };

        let mut frame_buf = [0u8; 127];
        let offset = &mut 0;
        frame_buf.write_with(
            offset,
            frame_header,
            &Some(&mut SecurityContext::no_security()),
        )?;
        let hdr_len = *offset;
        let payload_len = payload.len().min(frame_buf.len() - hdr_len - 2);
        frame_buf[hdr_len..hdr_len + payload_len].copy_from_slice(&payload[..payload_len]);
        // 2-byte FCS placeholder (IEEE 802.15.4 7.2.1.8) — the hardware
        // computes the actual CRC-16 over the frame and overwrites these
        // bytes during transmission
        let total_len = hdr_len + payload_len + 2;

        // retransmit unicasts per 7.5.6.4: a single CCA-busy or lost ack must
        // not drop a ZDO/APS response — the coordinator treats the silence as
        // an interview failure
        if is_broadcast {
            self.driver.transmit(&frame_buf[..total_len]).await?;
        } else {
            self.transmit_acked(&frame_buf[..total_len]).await?;
        }
        log::trace!("[MLME] tx data, len={total_len}");

        Ok(())
    }
}

impl Mlme for EspMlme<'_> {
    fn ieee_address(&self) -> zigbee_types::IeeeAddress {
        zigbee_types::IeeeAddress(self.ieee_address)
    }

    async fn configure(&self, config: MacConfig) {
        // returns `()`, so `Self::lock`'s error has nowhere to go — locks
        // unbounded and skips on queue-full instead.
        let Ok(mut inner) = self.inner.lock().await else {
            log::error!("[MAC] radio lock queue full, skipping configure");
            return;
        };
        inner.driver.update_driver_config(|driver| {
            if let Some(channel) = config.channel {
                driver.channel = channel;
            }
            if let Some(pan_id) = config.pan_id {
                driver.pan_id = Some(pan_id.0);
            }
            if let Some(short_address) = config.short_address {
                driver.short_addr = Some(short_address.0);
            }
            if let Some(promiscuous) = config.promiscuous {
                driver.promiscuous = promiscuous;
            }
            if let Some(auto_ack_rx) = config.auto_ack_rx {
                driver.auto_ack_rx = auto_ack_rx;
            }
            if let Some(auto_ack_tx) = config.auto_ack_tx {
                driver.auto_ack_tx = auto_ack_tx;
            }
        });
        // arm RX for the new configuration: otherwise reception stays off
        // until the next transmit triggers rx-on-when-idle
        inner.driver.start_receive();
    }

    async fn scan_network(
        &self,
        ty: ScanType,
        channels: core::ops::Range<u8>,
        duration: u8,
    ) -> Result<ScanResult, MacError> {
        self.lock()
            .await?
            .scan_network(ty, channels, duration)
            .await
    }

    async fn associate(
        &self,
        channel: u8,
        dest: Address,
        capabilities: CapabilityInformation,
    ) -> Result<AssociationResponse, MacError> {
        self.lock()
            .await?
            .associate(channel, dest, capabilities)
            .await
    }

    async fn poll_data(
        &self,
        coord_address: Address,
        buf: &mut [u8],
    ) -> Result<(usize, u8), MacError> {
        self.lock().await?.poll_data(coord_address, buf).await
    }

    async fn receive(&self, buf: &mut [u8]) -> Result<(usize, u8), MacError> {
        match self.receive_data(buf).await {
            Ok((len, lqi)) => {
                log::trace!("[MLME] rx data, len={len}");
                Ok((len, lqi))
            }
            Err(e) => Err(e),
        }
    }

    async fn transmit_data(&self, dest: Address, payload: &[u8]) -> Result<(), MacError> {
        self.lock().await?.transmit_data(dest, payload).await
    }
}
