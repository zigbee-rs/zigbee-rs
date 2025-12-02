use alloc::vec::Vec;

use byte::BytesExt;
use embassy_futures::select::Either;
use embassy_time::Timer;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;
use ieee802154::mac::Address;
use ieee802154::mac::FrameContent;
use ieee802154::mac::Header;

use crate::esp::driver::Ieee802154Driver;
use crate::mlme::A_BASE_SUPER_FRAME_DURATION;
use crate::mlme::MAX_IEEE802154_CHANNELS;
use crate::mlme::MAX_PAN_DESCRIPTOR_SIZE;
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

    const fn beacon_request_frame(&self) -> [u8; 10] {
        let seq_number = self.seq_number;
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

        if let Err(e) = self.driver.transmit(&self.beacon_request_frame()).await {
            log::error!("[MLME-SCAN]: error transmitting beacon: {e}");
        }

        log::debug!("[MLME-SCAN] sent beacon frame to channel {channel}, waiting for messages...");

        self.seq_number = self.seq_number.wrapping_add(1);

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
        for _ in 0..MAX_PAN_DESCRIPTOR_SIZE {
            let frame = self.driver.receive().await;
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
                    return Err(MacError::RadioError);
                }
            }
        }

        Ok(())
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
}
