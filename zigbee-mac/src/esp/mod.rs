use alloc::vec::Vec;

use byte::BytesExt;
use embassy_futures::select::Either;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
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
use crate::mlme::A_BASE_SUPER_FRAME_DURATION;
use crate::mlme::A_MAX_FRAME_RETRIES;
use crate::mlme::A_RESPONSE_WAIT_TIME;
use crate::mlme::AssociationResponse;
use crate::mlme::MAX_IEEE802154_CHANNELS;
use crate::mlme::MacError;
use crate::mlme::Mlme;
use crate::mlme::PanDescriptor;
use crate::mlme::PanDescriptorList;
use crate::mlme::ScanResult;
use crate::mlme::ScanType;

mod driver;

/// Higher-layer retries of the whole association handshake. Frame-level
/// `aMaxFrameRetries` ack-retransmission already covers a lost request or poll;
/// this is a safety net for a parent that accepts the request but is slow to
/// make the response available.
const ASSOCIATE_REQUEST_RETRIES: u8 = 3;

/// Number of times the association response is polled per request attempt.
const ASSOCIATE_POLL_RETRIES: u8 = 5;

/// Formats an optional MAC address as `0x..` for consistent logging.
struct AddrHex<'a>(&'a Option<Address>);

impl core::fmt::Display for AddrHex<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(Address::Short(_, addr)) => write!(f, "0x{:04x}", addr.0),
            Some(Address::Extended(_, addr)) => write!(f, "0x{:016x}", addr.0),
            _ => write!(f, "none"),
        }
    }
}

/// ESP32-C6 [`Mlme`] implementation.
///
/// The radio is a single shared resource: the inner state is held behind an
/// async mutex so the trait's `&self` methods can be driven concurrently from a
/// receive task and a transmit path. The extended address is cached so it can
/// be read without locking.
pub struct EspMlme<'a> {
    inner: Mutex<CriticalSectionRawMutex, EspMlmeInner<'a>>,
    ieee_address: u64,
}

impl<'a> EspMlme<'a> {
    pub fn new(ieee802154: Ieee802154<'a>, config: Config) -> Self {
        // Seed the MAC sequence number randomly (IEEE 802.15.4 §7.2.1.2). A fixed
        // start means every reboot re-uses the same low numbers, which a parent's
        // duplicate-frame filter (keyed by source + last-seen sequence) treats as
        // stale: it MAC-acks the frame in hardware but silently drops it before
        // the NWK layer, so an association request never gets a response.
        let inner = EspMlmeInner {
            driver: Ieee802154Driver::new(ieee802154, config),
            seq_number: (esp_hal::rng::Rng::new().random() & 0xff) as u8,
        };
        let ieee_address = inner.driver.ieee_address().0;
        Self {
            inner: Mutex::new(inner),
            ieee_address,
        }
    }

    /// The device's IEEE 802.15.4 extended (EUI-64) address.
    pub fn ieee_address(&self) -> u64 {
        self.ieee_address
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

    /// Transmit an acknowledgment-requested frame, retransmitting up to
    /// `aMaxFrameRetries` times if no acknowledgment is received
    /// (IEEE 802.15.4 §7.5.6.4). Returns [`MacError::NoAck`] if every attempt
    /// goes unacknowledged.
    async fn transmit_acked(&mut self, frame: &[u8]) -> Result<(), MacError> {
        for _ in 0..=A_MAX_FRAME_RETRIES {
            self.driver.transmit(frame).await?;
            if self.driver.last_tx_acked() {
                return Ok(());
            }
        }
        Err(MacError::NoAck)
    }

    /// Discard all buffered frames from the hardware RX queue.
    fn flush(&mut self) {
        while self.driver.poll_received().is_some() {}
    }

    /// Take the next frame from the hardware RX queue (non-blocking), mapping a
    /// radio error into [`MacError`]. Returns `None` when the queue is empty.
    fn poll_frame(&mut self) -> Option<Result<ReceivedFrame, MacError>> {
        self.driver
            .poll_received()
            .map(|r| r.map_err(MacError::RadioError))
    }

    /// Wait for the next frame from the hardware RX queue (indefinite).
    async fn next_frame(&mut self) -> Result<ReceivedFrame, MacError> {
        loop {
            // Wait for the radio to signal a fully-received, queued frame BEFORE
            // draining. Draining goes through the radio's `received()`, which
            // re-issues `RxStart`; doing that while a frame is still on the air
            // aborts it — this is how a fast indirect response (~2 ms after a
            // poll ack) was being dropped. RX stays armed via rx-on-when-idle, so
            // we never need to drain mid-reception. The signal is level-held, so
            // once set we keep draining until the queue is empty, then reset and
            // wait for the next signal.
            self.driver.wait_rx_available().await;
            if let Some(result) = self.poll_frame() {
                return result;
            }
            self.driver.reset_rx_signal();
        }
    }

    /// Drain the hardware RX queue, returning the payload + LQI of the first
    /// MAC data frame found (non-blocking). Non-data frames are discarded.
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

    /// Build a MAC data request command frame (IEEE 802.15.4 §7.3.4).
    ///
    /// Uses the assigned short address as source when available (i.e.
    /// after a successful association), otherwise falls back to the
    /// extended address.
    /// Build a data request command. Returns the buffer and the on-air length:
    /// the written bytes plus 2 for the FCS the radio appends (the PHR length
    /// must cover the FCS). The source address mode varies (extended before a
    /// short address is assigned, short after), so the length is not fixed and
    /// the caller must transmit exactly `&buf[..len]` — transmitting the whole
    /// fixed-size array would append junk bytes and the parent would reject the
    /// data request, never delivering the buffered (indirect) response.
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

    /// Listen up to `timeout_us` for a MAC association response, draining and
    /// discarding any other frame. Returns `None` on timeout. Does not flush,
    /// so a response already queued (a parent that sent it directly) is not
    /// lost.
    async fn recv_association_response(
        &mut self,
        timeout_us: u64,
    ) -> Result<Option<AssociationResponse>, MacError> {
        // Poll the RX queue on a fixed interval, WAITING before each drain. The
        // indirect response lands ~2 ms after the poll ack; the radio's RX is
        // already armed (rx-on-when-idle re-arm in the ack ISR), so waiting lets
        // it receive the response into the queue, and we then drain it. We must
        // not drain (which goes through `received()` and re-issues `RxStart`)
        // while a frame is still on the air — that aborts it — so the wait comes
        // first and the interval (~4 ms) exceeds a frame's ~0.8 ms air time. Time
        // based rather than signal based: the RX-available signal is not raised
        // if the auto-ack path does not complete, but the frame is still queued.
        // No logging in the loop — the UART critical section stalls the RX ISR.
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

/// Copy a MAC data frame's payload + LQI into `buf`, returning the byte count
/// and LQI. Non-data frames (commands, beacons, acks) yield `None`.
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
        // Filter on our extended address (forced into the hw filter): the
        // association response is an indirect transmission addressed to it
        // (IEEE 802.15.4 §7.5.6.4). auto_ack_rx must stay on so we ack the
        // response — promiscuous mode suppresses that ack and drowns the RX
        // queue in ambient broadcast traffic, dropping the response.
        self.driver.update_driver_config(|config| {
            *config = Default::default();
            config.channel = channel;
            config.pan_id = Some(dest.pan_id().0);
            config.auto_ack_tx = true;
            config.auto_ack_rx = true;
            config.promiscuous = false;
        });
        // arm the receiver after the channel/config change (as the scan path
        // does): otherwise general RX is not enabled until a TX later triggers
        // rx-on-when-idle, and the indirect association response that arrives
        // right after the poll ack is never received
        self.driver.start_receive();

        let ext_addr = self.driver.ieee_address();
        // IEEE 802.15.4 §7.3.1.1: the association request source PAN identifier
        // shall be the broadcast PAN (0xffff); the device is not yet a member of
        // the target PAN. Source address is the extended address.
        let src = Address::Extended(PanId::broadcast(), ext_addr);
        let timeout_us = (A_RESPONSE_WAIT_TIME as u64) * 16;

        // Retry the full association handshake (IEEE 802.15.4 §7.5.3.1). The
        // request itself can be lost or go unacked, in which case the parent
        // never buffers a response and polling cannot recover, so re-send the
        // whole request each round. Within a round, listen first — an
        // rx-on-when-idle parent sends the response directly, right after acking
        // the request — then poll (data request) to prompt indirect delivery.
        // Never flush: a directly-sent response may already be queued.
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
                // Arm RX for the buffered (indirect) response, which arrives
                // ~2-3 ms after the poll ack. start_receive() is safe here: it is
                // a no-op if the radio is already receiving and never aborts an
                // in-flight frame (unlike draining via received(), which forces
                // RxStart). Do not log in this window — the UART writer runs in a
                // critical section and stalls the RX ISR. A TI/Z-Stack parent
                // sends the response only once, so a single miss fails the round.
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

        // Step 5: Configure the assigned short address on the driver and
        // disable promiscuous mode so the hardware filter accepts
        // unicast frames addressed to us.
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
        self.flush();
        let (data_req, len) = self.data_request_frame(coord_address)?;
        // a missing ack means the parent has nothing buffered; still listen
        match self.transmit_acked(&data_req[..len]).await {
            Ok(()) | Err(MacError::NoAck) => {}
            Err(e) => return Err(e),
        }
        // arm RX for the response (safe no-op if already receiving); no log in
        // this window — the UART writer's critical section stalls the RX ISR and
        // the response arrives ~2 ms after the poll ack
        self.driver.start_receive();

        let timeout_us = (A_RESPONSE_WAIT_TIME as u64) * 16;
        let timeout = Timer::after_micros(timeout_us);
        let receive = async {
            loop {
                let received = self.next_frame().await?;
                // ignore ambient broadcasts (link-status, route-request, etc.)
                // sharing the listen window: a poll response is always a unicast
                // addressed to this device. Returning early on a broadcast would
                // let the next flush() discard the still-buffered unicast.
                if matches!(
                    received.frame.header.destination,
                    Some(Address::Short(_, ieee802154::mac::ShortAddress(d))) if d >= 0xfff8
                ) {
                    log::debug!("[MLME-POLL] skip broadcast in poll window");
                    continue;
                }
                if let Some((len, lqi)) = copy_data_payload(received, buf) {
                    log::debug!("[MLME-POLL] rx data len={len}");
                    return Ok((len, lqi));
                }
            }
        };
        match embassy_futures::select::select(timeout, receive).await {
            Either::First(_) => Err(MacError::NoData),
            Either::Second(result) => result,
        }
    }

    async fn transmit_data(&mut self, dest: Address, payload: &[u8]) -> Result<(), MacError> {
        let seq = self.sequence_number();

        // NWK broadcast addresses (0xfff8-0xffff) map to the MAC broadcast
        // address 0xffff, which is never acknowledged (IEEE 802.15.4 §7.2.1.1.2)
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
        // 2-byte FCS placeholder (IEEE 802.15.4 §7.2.1.8) — the hardware
        // computes the actual CRC-16 over the frame and overwrites these
        // bytes during transmission
        let total_len = hdr_len + payload_len + 2;

        self.driver.transmit(&frame_buf[..total_len]).await?;
        log::debug!("[MLME] tx data, len={total_len}");

        Ok(())
    }
}

impl Mlme for EspMlme<'_> {
    async fn scan_network(
        &self,
        ty: ScanType,
        channels: core::ops::Range<u8>,
        duration: u8,
    ) -> Result<ScanResult, MacError> {
        self.inner
            .lock()
            .await
            .scan_network(ty, channels, duration)
            .await
    }

    async fn associate(
        &self,
        channel: u8,
        dest: Address,
        capabilities: CapabilityInformation,
    ) -> Result<AssociationResponse, MacError> {
        self.inner
            .lock()
            .await
            .associate(channel, dest, capabilities)
            .await
    }

    async fn poll_data(
        &self,
        coord_address: Address,
        buf: &mut [u8],
    ) -> Result<(usize, u8), MacError> {
        self.inner.lock().await.poll_data(coord_address, buf).await
    }

    async fn receive(&self, buf: &mut [u8]) -> Result<(usize, u8), MacError> {
        loop {
            // drain under a brief lock, then idle-wait lock-free so a concurrent
            // transmit can acquire the radio while we wait for the next frame
            {
                let mut inner = self.inner.lock().await;
                if let Some(received) = inner.try_drain(buf)? {
                    return Ok(received);
                }
            }
            driver::wait_rx_signal().await;
        }
    }

    async fn transmit_data(&self, dest: Address, payload: &[u8]) -> Result<(), MacError> {
        self.inner.lock().await.transmit_data(dest, payload).await
    }
}
