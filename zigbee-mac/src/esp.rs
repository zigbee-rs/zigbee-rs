use core::mem;

use byte::BytesExt;
use esp_hal::delay::Delay;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;
use esp_sync::NonReentrantMutex;
use ieee802154::mac::Address;
use ieee802154::mac::FrameContent;
use ieee802154::mac::Header;

use crate::mlme::MacError;
use crate::mlme::Mlme;
use crate::mlme::PanDescriptor;
use crate::mlme::ScanResult;
use crate::mlme::ScanType;
use crate::mlme::A_BASE_SUPER_FRAME_DURATION;
use crate::mlme::MAX_IEEE802154_CHANNELS;

pub struct EspMlme<'a> {
    ieee802154: Ieee802154<'a>,
    seq_number: u8,
}

impl<'a> EspMlme<'a> {
    pub fn new(ieee802154: Ieee802154<'a>) -> Self {
        Self {
            //ieee802154: NonReentrantMutex::new(ieee802154),
            ieee802154,
            seq_number: 0,
        }
    }

    const fn beacon_request_frame(&self) -> [u8; 10] {
        let seq_number = self.seq_number;
        [0x3, 0x8, seq_number, 0xff, 0xff, 0xff, 0xff, 0x7, 0x0, 0x0]
    }

    fn scan_channel_active(
        &mut self,
        channel: u8,
        duration: u8,
    ) -> Result<Option<PanDescriptor>, MacError> {
        let delay = Delay::new();
        let config = Config {
            channel,
            txpower: 20,
            promiscuous: false,
            short_addr: Some(0xdead),
            pan_id: Some(6754),
            ..Default::default()
        };
        self.ieee802154.set_config(config);
        let tx_done = NonReentrantMutex::new(false);
        let f: &mut (dyn FnMut() + Send) = &mut || {
            tx_done.with(|t| *t = true);
            log::debug!(
                "[MLME-SCAN] sent beacon frame to channel {channel}, waiting for messages..."
            );
            self.ieee802154.start_receive();
        };
        let f: &'static mut (dyn FnMut() + Send) = unsafe { mem::transmute(f) };
        self.ieee802154.set_tx_done_callback(f);

        if let Err(e) = self.ieee802154.transmit_raw(&self.beacon_request_frame()) {
            log::error!("[MLME-SCAN]: error transmitting beacon: {e}");
        }

        self.seq_number = self.seq_number.wrapping_add(1);

        let mut is_sending = true;
        while is_sending {
            tx_done.with(|t| is_sending = !*t);
        }

        // TODO: we should return before the delay is elapsed if a frame is received
        let delay_us = calculate_scan_duration_max_us(duration);
        log::debug!("[MLME-SCAN] waiting for response for {delay_us}us");

        delay.delay_micros(delay_us);

        // check the rx queue
        let mut scan_result = None;
        while let Some(res) = self.ieee802154.received() {
            log::debug!("ieee802154 receive");
            match res {
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

                    scan_result = Some(PanDescriptor {
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
                    });
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

        if scan_result.is_none() {
            log::debug!("[MLME-SCAN] no response on channel {channel}");
        }

        Ok(scan_result)
    }
}

fn calculate_scan_duration_max_us(duration: u8) -> u32 {
    // we assume a symbol period of 16us (QPSK, 2.4Ghz)
    16 * A_BASE_SUPER_FRAME_DURATION * (2 * (duration as u32) + 1)
}

impl Mlme for EspMlme<'_> {
    fn scan_network(
        &mut self,
        scan_type: ScanType,
        channels: impl Iterator<Item = u8>,
        duration: u8,
    ) -> Result<ScanResult, MacError> {
        if !matches!(scan_type, ScanType::Active) {
            return Err(MacError::InvalidScanParams);
        }

        log::debug!("[MLME-SCAN] start scan");

        let pan_descriptor = channels
            .filter(|c| (*c as usize) < MAX_IEEE802154_CHANNELS)
            .filter_map(|c| match self.scan_channel_active(c, duration) {
                Ok(Some(pd)) => Some(pd),
                Err(e) => {
                    log::error!("[MLME-SCAN] error on channel {c}: {e}");
                    None
                }
                _ => None,
            })
            .collect();

        Ok(ScanResult {
            scan_type,
            pan_descriptor,
        })
    }
}
