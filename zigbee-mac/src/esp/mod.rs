use alloc::vec::Vec;

use byte::BytesExt;
use embassy_futures::select::Either;
use embassy_time::Timer;
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
}

impl<'a> EspMlme<'a> {
    pub fn new(ieee802154: Ieee802154<'a>, config: Config) -> Self {
        Self {
            driver: Ieee802154Driver::new(ieee802154, config),
            seq_number: 0,
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

    async fn wait_for_ack(&mut self) -> Result<(), MacError> {
        let mut receiver_stream = self.driver.stream();
        while let Some(frame) = receiver_stream.next().await {
            match frame {
                Ok(ReceivedFrame {
                    frame:
                        Frame {
                            header: Header { seq, .. },

                            content: FrameContent::Acknowledgement,
                            ..
                        },
                    ..
                }) if seq == self.seq_number => {
                    break;
                }
                Ok(_) => continue,
                Err(e) => return Err(MacError::RadioError(e)),
            }
        }

        Ok(())
    }

    fn association_request_frame(
        &self,
        dest: Address,
        src: Option<Address>,
        capabilities: CapabilityInformation,
    ) -> Result<[u8; 20], MacError> {
        let frame_header = Header {
            frame_type: FrameType::MacCommand,
            frame_pending: false,
            ack_request: true,
            pan_id_compress: false,
            seq_no_suppress: false,
            ie_present: false,
            version: FrameVersion::Ieee802154_2003,
            seq: self.seq_number,
            destination: Some(dest),
            source: src,
            auxiliary_security_header: None,
        };
        let frame_content = FrameContent::Command(Command::AssociationRequest(capabilities));

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
        self.driver.update_driver_config(|config| {
            *config = Default::default();
            config.channel = channel;
            config.pan_id = Some(dest.pan_id().0);
        });

        let frame = self.association_request_frame(dest, None, capabilities)?;
        self.driver.transmit(&frame).await?;

        // wait for ACK (within macResponseWaitTime)
        self.wait_for_ack().await?;

        // wait for response
        // generate MLME-ASSOCIATE.response

        unimplemented!()
    }
}
