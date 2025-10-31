use core::mem;

use esp_hal::delay::Delay;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Ieee802154;
use esp_sync::NonReentrantMutex;
use ieee802154::mac::FrameType;

use crate::MacError;
use crate::Mlme;
use crate::ScanResult;
use crate::A_BASE_SUPER_FRAME_DURATION;
use crate::MAX_IEEE802154_CHANNELS;

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

    fn scan_channel_active(
        &mut self,
        channel: u8,
        duration: u8,
    ) -> Result<Option<ScanResult>, MacError> {
        let delay = Delay::new();
        let config = Config {
            channel,
            txpower: 20,
            promiscuous: false,
            //auto_ack_tx: true,
            //auto_ack_rx: true,
            //rx_when_idle: true,
            short_addr: Some(0xdead),
            pan_id: Some(6754),
            ..Default::default()
        };
        self.ieee802154.set_config(config);
        let tx_done = NonReentrantMutex::new(false);
        let f: &mut (dyn FnMut() + Send) = &mut || {
            tx_done.with(|t| *t = true);
            log::info!(
                "[MLME-SCAN] sent beacon frame to channel {channel}, waiting for messages..."
            );
            self.ieee802154.start_receive();
        };
        let f: &'static mut (dyn FnMut() + Send) = unsafe { mem::transmute(f) };
        self.ieee802154.set_tx_done_callback(f);

        if let Err(e) = self.ieee802154.transmit_raw(&[
            0x3,
            0x8,
            self.seq_number,
            0xff,
            0xff,
            0xff,
            0xff,
            0x7,
            0x0,
            0x0,
        ]) {
            log::error!("[MLME-SCAN]: error transmitting beacon: {e}");
        }

        self.seq_number = self.seq_number.wrapping_add(1);

        let mut is_sending = true;
        while is_sending {
            tx_done.with(|t| is_sending = !*t);
        }

        // TODO: we should return before the delay is elapsed if a frame is received
        let delay_us = calculate_scan_duration_max_us(duration);
        log::info!("[MLME-SCAN] waiting for response for {delay_us}us");

        delay.delay_micros(delay_us);

        // check the rx queue
        let mut scan_result = None;
        while let Some(res) = self.ieee802154.received() {
            match res {
                Ok(frame) if frame.frame.header.frame_type == FrameType::Beacon => {
                    log::info!(
                        "[MLME-SCAN] receive frame on channel {channel}: {frame:#?}",
                        frame = frame.frame,
                    );

                    scan_result = Some(ScanResult { channel });
                }
                Ok(_) => {}
                Err(e) => {
                    log::error!("[MLME-SCAN] receive error: {e}");
                }
            }
        }

        if scan_result.is_none() {
            log::info!("[MLME-SCAN] no response on channel {channel}");
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
        _ty: crate::ScanType,
        channels: u32,
        duration: u8,
    ) -> Result<(), MacError> {
        log::info!("[MLME-SCAN] start scan");

        for channel in 11..MAX_IEEE802154_CHANNELS {
            let _ = self.scan_channel_active(channel, duration);
        }

        Ok(())
    }
}
