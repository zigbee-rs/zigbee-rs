use alloc::vec::Vec;

use byte::BytesExt;
use embassy_futures::select::Either;
use embassy_time::Timer;
use esp_hal::efuse::Efuse;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;
use futures_util::StreamExt;
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

    fn beacon_request_frame(&mut self) -> [u8; 10] {
        let seq_number = self.sequence_number();
        [0x3, 0x8, seq_number, 0xff, 0xff, 0xff, 0xff, 0x7, 0x0, 0x0]
    }

    async fn scan_channel_active(
        &mut self,
        channel: u8,
        duration: u8,
    ) -> Result<Option<PanDescriptorList>, MacError> {
        self.driver.update_driver_config(|config| {
            config.promiscuous = false;
            config.channel = channel;
        });

        let frame = self.beacon_request_frame();
        if let Err(e) = self.driver.transmit(&frame).await {
            log::error!("[MLME-SCAN]: error transmitting beacon: {e}");
        }

        log::debug!("[MLME-SCAN] sent beacon frame to channel {channel}, waiting for messages...");

        let delay_us = calculate_scan_duration_max_us(duration);
        log::debug!("[MLME-SCAN] waiting for response for {delay_us}us");

        let mut pds = Vec::new();

        let timer_fut = Timer::after_micros(delay_us.into());
        let receive_fut = self.receive_beacon(channel, &mut pds);
        let sel = embassy_futures::select::select(timer_fut, receive_fut).await;
        match sel {
            Either::First(_) => Ok(Some(pds)),
            Either::Second(Ok(_)) => Ok(Some(pds)),
            Either::Second(Err(e)) => Err(e),
        }
    }

    async fn receive_beacon(
        &mut self,
        channel: u8,
        pds: &mut PanDescriptorList,
    ) -> Result<(), MacError> {
        let mut receiver_stream = self.driver.stream();
        while let Some(frame) = receiver_stream.next().await {
            match frame {
                Ok(ReceivedFrame {
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
                }) => {
                    log::debug!("[MLME-SCAN] received beacon frame on channel {channel}");

                    let zigbee_beacon =
                        payload.read_with(&mut 0, ()).map_err(MacError::ReadError)?;

                    let pd = PanDescriptor {
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
                    };
                    pds.push(pd);
                }
                Ok(f) => {
                    log::debug!("[MLME-SCAN] received other frame on channel {channel}: {f:?}");
                }
                Err(e) => {
                    log::error!("[MLME-SCAN] receive error: {e}");
                    return Err(MacError::RadioError(e));
                }
            }
        }

        Ok(())
    }

    async fn wait_for_ack(&mut self) -> Result<bool, MacError> {
        let mut receiver_stream = self.driver.stream();
        while let Some(frame) = receiver_stream.next().await {
            match frame {
                Ok(ReceivedFrame {
                    frame:
                        Frame {
                            header:
                                Header {
                                    seq, frame_pending, ..
                                },

                            content: FrameContent::Acknowledgement,
                            ..
                        },
                    ..
                }) if seq == self.seq_number => {
                    return Ok(frame_pending);
                }
                Ok(_) => continue,
                Err(e) => return Err(MacError::RadioError(e)),
            }
        }

        Err(MacError::NoAck)
    }

    /// Build a MAC data request command frame (IEEE 802.15.4 §7.3.2.1).
    ///
    /// After an association request, the device polls the coordinator for
    /// the association response by sending this command.
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

    /// Wait for an association response command frame from the coordinator.
    ///
    /// Per IEEE 802.15.4 §7.5.3.1, after the data request is acknowledged
    /// with frame_pending=1, the coordinator sends the association response
    /// command containing the assigned short address and status.
    ///
    /// If no response is received within `aResponseWaitTime` symbols, the
    /// primitive returns `MacError::NoData` (IEEE 802.15.4 §7.1.3.1.3).
    async fn wait_for_association_response(&mut self) -> Result<AssociationResponse, MacError> {
        let timeout = Timer::after_micros((A_RESPONSE_WAIT_TIME as u64) * 16);
        let receive = async {
            let mut receiver_stream = self.driver.stream();
            while let Some(frame) = receiver_stream.next().await {
                log::info!("[MLME-ASSOCIATE] received frame {frame:?}");
                match frame {
                    Ok(ReceivedFrame {
                        frame:
                            Frame {
                                header:
                                    Header {
                                        source: Some(source),
                                        ..
                                    },
                                content:
                                    FrameContent::Command(Command::AssociationResponse(
                                        short_addr,
                                        status,
                                    )),
                                ..
                            },
                        ..
                    }) => {
                        let device_address = match source {
                            Address::Extended(_, ext) => ext,
                            Address::Short(_, _) => {
                                log::warn!(
                                    "[MLME-ASSOCIATE] expected extended address in association response"
                                );
                                continue;
                            }
                        };
                        return Ok(AssociationResponse {
                            device_address,
                            association_address: zigbee_types::ShortAddress(short_addr.0),
                            status,
                        });
                    }
                    Ok(_) => continue,
                    Err(e) => return Err(MacError::RadioError(e)),
                }
            }

            Err(MacError::NoData)
        };

        match embassy_futures::select::select(timeout, receive).await {
            Either::First(_) => {
                log::warn!("[MLME-ASSOCIATE] timed out waiting for association response");
                Err(MacError::NoData)
            }
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
            // Tell the hardware our extended address so that address
            // filtering and auto-ACK work for frames addressed to us.
            config.ext_addr = Some(ext_addr.0);
            // Enable hardware auto-ACK for received frames addressed to us.
            // The coordinator's association response has ack_request set, and
            // per IEEE 802.15.4 §7.5.3.1 we must acknowledge it. ACK timing
            // is 12 symbol periods (~192 µs) which is too tight for software.
            config.auto_ack_rx = true;
        });

        // Step 1: Send association request command (IEEE 802.15.4 §7.5.3.1).
        // The source address must be the device's extended address.
        let src = Address::Extended(dest.pan_id(), ext_addr);
        let frame = self.association_request_frame(dest, Some(src), capabilities)?;
        self.driver.transmit(&frame).await?;
        log::info!("[MLME-ASSOCIATE] request transmitted");

        // Step 2: Wait for ACK to the association request
        //let _frame_pending = self.wait_for_ack().await?;
        //log::info!("[MLME-ASSOCIATE] ack received");

        // Step 3: Wait aResponseWaitTime for the coordinator to prepare its
        // association decision (IEEE 802.15.4 §7.5.3.1).
        // aResponseWaitTime = 32 * aBaseSuperframeDuration symbols.
        // At 16µs/symbol (2.4 GHz O-QPSK): ~491 ms.
        let wait_us: u64 = (A_RESPONSE_WAIT_TIME as u64) * 16;
        Timer::after_micros(wait_us).await;
        log::info!("[MLME-ASSOCIATE] waited for {wait_us} us");

        // Step 4–5: Poll the coordinator for the association response
        // (MLME-POLL.request per IEEE 802.15.4 §7.1.16.1).
        self.poll(dest).await?;
        //log::info!("[MLME-ASSOCIATE] data poll requested");

        // Step 6: Receive the association response command frame from the
        // coordinator containing the assigned short address and status.
        // The ACK for the response is sent automatically by the hardware
        // (auto_ack_rx = true) per IEEE 802.15.4 §7.5.3.1.
        // If no response arrives within aResponseWaitTime, NoData is returned.
        self.wait_for_association_response().await
    }

    async fn poll(&mut self, coord_address: Address) -> Result<(), MacError> {
        // Send a data request command to poll the coordinator for pending
        // data (IEEE 802.15.4 §7.1.16.1.3, §7.3.2.1).
        let data_req = self.data_request_frame(coord_address, self.ieee_address)?;
        self.driver.transmit_and_listen(&data_req).await?;

        Ok(())
    }

    async fn receive(&mut self, buf: &mut [u8]) -> Result<(usize, u8), MacError> {
        let timeout = Timer::after_micros((A_RESPONSE_WAIT_TIME as u64) * 16);
        let receive = async {
            let mut stream = self.driver.stream();
            while let Some(frame) = stream.next().await {
                match frame {
                    Ok(ReceivedFrame {
                        frame:
                            Frame {
                                content: FrameContent::Data,
                                payload,
                                ..
                            },
                        lqi,
                        ..
                    }) => {
                        let len = payload.len().min(buf.len());
                        buf[..len].copy_from_slice(&payload[..len]);
                        return Ok((len, lqi));
                    }
                    Ok(_) => continue,
                    Err(e) => return Err(MacError::RadioError(e)),
                }
            }
            Err(MacError::NoData)
        };

        match embassy_futures::select::select(timeout, receive).await {
            Either::First(_) => Err(MacError::NoData),
            Either::Second(result) => result,
        }
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
        // bytes during transmission.
        let total_len = hdr_len + payload_len + 2;

        self.driver.transmit(&frame_buf[..total_len]).await?;
        self.wait_for_ack().await?;
        Ok(())
    }
}
