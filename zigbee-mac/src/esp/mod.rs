use alloc::vec::Vec;

use byte::BytesExt;
use embassy_futures::select::Either;
use embassy_time::Timer;
use esp_hal::efuse::Efuse;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;
use ieee802154::mac::Address;
use ieee802154::mac::FrameContent;
use ieee802154::mac::FrameType;
use ieee802154::mac::FrameVersion;
use ieee802154::mac::Header;
use ieee802154::mac::command::CapabilityInformation;
use ieee802154::mac::command::Command;
use ieee802154::mac::security::SecurityContext;

use crate::esp::driver::Ieee802154Driver;
use crate::mlme::A_BASE_SUPER_FRAME_DURATION;
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

/// Wait for the first frame matching `$pat` within `$timeout_us` microseconds,
/// skipping non-matching frames. Evaluates `$body` on match. Returns
/// `Err(MacError::NoData)` if the timeout expires before a match.
macro_rules! recv_frame {
    ($self:expr, $timeout_us:expr, $($pat:pat => $body:expr),+ $(,)?) => {{
        let timeout = Timer::after_micros($timeout_us);
        let receive = async {
            loop {
                let frame = $self.next_frame().await?;
                match frame {
                    $($pat => return Ok($body),)+
                    _ => continue,
                }
            }
        };
        match embassy_futures::select::select(timeout, receive).await {
            Either::First(_) => Err(MacError::NoData),
            Either::Second(result) => result,
        }
    }};
}

pub struct EspMlme<'a> {
    driver: Ieee802154Driver<'a>,
    seq_number: u8,
    /// IEEE 802.15.4 extended (EUI-64) address derived from the
    /// ESP32's factory-burned eFuse MAC address.
    ieee_address: ieee802154::mac::ExtendedAddress,
}

/// Derive an EUI-64 extended address from a 6-byte EUI-48 MAC.
///
/// Inserts `0xFF, 0xFE` after the OUI (first 3 bytes) per the
/// IEEE EUI-64 conversion convention:
///   `AA:BB:CC:DD:EE:FF` → `AA:BB:CC:FF:FE:DD:EE:FF`
fn eui48_to_eui64(mac: [u8; 6]) -> u64 {
    u64::from_be_bytes([mac[0], mac[1], mac[2], 0xFF, 0xFE, mac[3], mac[4], mac[5]])
}

impl<'a> EspMlme<'a> {
    pub fn new(ieee802154: Ieee802154<'a>, config: Config) -> Self {
        let ieee_address = ieee802154::mac::ExtendedAddress(eui48_to_eui64(Efuse::mac_address()));
        Self {
            driver: Ieee802154Driver::new(ieee802154, config),
            seq_number: 0,
            ieee_address,
        }
    }
}

impl EspMlme<'_> {
    fn sequence_number(&mut self) -> u8 {
        self.seq_number = self.seq_number.wrapping_add(1);
        self.seq_number
    }

    /// Discard all buffered frames from the hardware RX queue.
    fn flush(&mut self) {
        while self.driver.poll_received().is_some() {}
    }

    /// Wait for the next frame from the hardware RX queue (indefinite).
    async fn next_frame(&mut self) -> Result<ReceivedFrame, MacError> {
        loop {
            if let Some(result) = self.driver.poll_received() {
                return result.map_err(MacError::RadioError);
            }
            self.driver.wait_rx_available().await;
        }
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
        let timeout = Timer::after_micros(delay_us);
        let receive = async {
            loop {
                let frame = self.next_frame().await?;
                if let Some(pd) = self.parse_beacon(frame) {
                    pds.push(pd);
                }
            }
        };
        // collect beacons until the scan duration expires
        let _ = embassy_futures::select::select(timeout, receive).await;

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
                        log::warn!("[MLME-SCAN] failed to parse zigbee beacon: {e}");
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

    /// Build a MAC data request command frame (IEEE 802.15.4 §7.3.2.1).
    fn data_request_frame(
        &mut self,
        dest: Address,
        src_ieee: ieee802154::mac::ExtendedAddress,
    ) -> Result<[u8; 20], MacError> {
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
            source: Some(Address::Extended(dest.pan_id(), src_ieee)),
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

        Ok(buf)
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

fn calculate_scan_duration_max_us(duration: u8) -> u32 {
    // we assume a symbol period of 16us (QPSK, 2.4Ghz)
    16 * A_BASE_SUPER_FRAME_DURATION * (2 * (duration as u32) + 1)
}

impl Mlme for EspMlme<'_> {
    async fn scan_network(
        &mut self,
        scan_type: ScanType,
        channels: impl Iterator<Item = u8>,
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
        let ext_addr = self.ieee_address;
        self.driver.update_driver_config(|config| {
            *config = Default::default();
            config.channel = channel;
            config.pan_id = Some(dest.pan_id().0);
            config.ext_addr = Some(ext_addr.0);
            config.auto_ack_rx = true;
        });

        // Step 1: Send association request command (IEEE 802.15.4 §7.5.3.1).
        let src = Address::Extended(dest.pan_id(), ext_addr);
        let frame = self.association_request_frame(dest, Some(src), capabilities)?;
        self.driver.transmit(&frame).await?;
        log::info!("[MLME-ASSOCIATE] request transmitted");

        // Step 2: Wait aResponseWaitTime for the coordinator to prepare its
        // association decision (IEEE 802.15.4 §7.5.3.1).
        let wait_us: u64 = (A_RESPONSE_WAIT_TIME as u64) * 16;
        Timer::after_micros(wait_us).await;
        log::info!("[MLME-ASSOCIATE] waited for {wait_us} us");

        // Step 3: Poll the coordinator for the association response.
        self.flush();
        let data_req = self.data_request_frame(dest, self.ieee_address)?;
        self.driver.transmit(&data_req).await?;

        // Step 4: Wait for the association response command frame.
        let timeout_us = (A_RESPONSE_WAIT_TIME as u64) * 16;
        recv_frame!(self, timeout_us,
            ReceivedFrame {
                frame: Frame {
                    header: Header { source: Some(Address::Extended(_, ext)), .. },
                    content: FrameContent::Command(
                        Command::AssociationResponse(short_addr, status),
                    ),
                    ..
                },
                ..
            } => AssociationResponse {
                device_address: ext,
                association_address: zigbee_types::ShortAddress(short_addr.0),
                status,
            },
        )
    }

    async fn poll_data(
        &mut self,
        coord_address: Address,
        buf: &mut [u8],
    ) -> Result<(usize, u8), MacError> {
        self.flush();
        let data_req = self.data_request_frame(coord_address, self.ieee_address)?;
        self.driver.transmit(&data_req).await?;

        let timeout_us = (A_RESPONSE_WAIT_TIME as u64) * 16;
        recv_frame!(self, timeout_us,
            ReceivedFrame {
                frame: Frame { content: FrameContent::Data, payload, .. },
                lqi,
                ..
            } => {
                let len = payload.len().min(buf.len());
                buf[..len].copy_from_slice(&payload[..len]);
                (len, lqi)
            },
        )
    }

    async fn transmit_data(&mut self, dest: Address, payload: &[u8]) -> Result<(), MacError> {
        let seq = self.sequence_number();
        let source = Some(Address::Extended(dest.pan_id(), self.ieee_address));

        let frame_header = Header {
            frame_type: FrameType::Data,
            frame_pending: false,
            ack_request: true,
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
        Ok(())
    }
}
