use esp_hal::delay::Delay;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use ieee802154::mac::beacon::Beacon;
use ieee802154::mac::beacon::BeaconOrder;
use ieee802154::mac::beacon::GuaranteedTimeSlotInformation;
use ieee802154::mac::beacon::PendingAddress;
use ieee802154::mac::beacon::SuperframeOrder;
use ieee802154::mac::beacon::SuperframeSpecification;
use ieee802154::mac::Address;
use ieee802154::mac::AddressMode;
use ieee802154::mac::FrameContent;
use ieee802154::mac::FrameType;
use ieee802154::mac::FrameVersion;
use ieee802154::mac::Header;

use crate::MacError;
use crate::Mlme;
use crate::ScanResult;
use crate::A_BASE_SUPER_FRAME_DURATION;
use crate::MAX_IEEE802154_CHANNELS;

pub struct EspMlme<'a> {
    ieee802154: Ieee802154<'a>,
}

impl<'a> EspMlme<'a> {
    pub fn new(ieee802154: Ieee802154<'a>) -> Self {
        Self {
            //ieee802154: NonReentrantMutex::new(ieee802154),
            ieee802154,
        }
    }

    fn scan_channel_active(
        &mut self,
        channel: u8,
        duration: u8,
    ) -> Result<Option<ScanResult>, MacError> {
        let config = Config {
            channel,
            txpower: 20,
            promiscuous: false,
            auto_ack_tx: true,
            auto_ack_rx: true,
            rx_when_idle: true,
            short_addr: Some(0xdead),
            ..Default::default()
        };
        self.ieee802154.set_config(config);

        let beacon_frame = Frame {
            header: Header {
                frame_type: FrameType::Beacon,
                frame_pending: false,
                ack_request: false,
                pan_id_compress: false,
                seq_no_suppress: false,
                ie_present: false,
                version: FrameVersion::Ieee802154_2003,
                seq: 0u8,
                destination: Address::broadcast(&AddressMode::Short),
                source: None,
                auxiliary_security_header: None,
            },
            content: FrameContent::Beacon(Beacon {
                superframe_spec: SuperframeSpecification {
                    beacon_order: BeaconOrder::OnDemand,
                    superframe_order: SuperframeOrder::Inactive,
                    final_cap_slot: 0,
                    battery_life_extension: false,
                    pan_coordinator: false,
                    association_permit: false,
                },
                guaranteed_time_slot_info: GuaranteedTimeSlotInformation::new(),
                pending_address: PendingAddress::new(),
            }),
            payload: [].to_vec(),
            footer: [0u8; 2],
        };

        //let _ = self.ieee802154.transmit(&beacon_frame);
        if let Err(e) = self
            .ieee802154
            .transmit_raw(&[0x3, 0x8, 0x4f, 0xff, 0xff, 0xff, 0xff, 0x7])
        {
            log::error!("[MLME-SCAN]: error transmitting beacon: {e}");
        }

        // TODO: we should return before the delay is elapsed if a frame is received
        let delay_us = calculate_scan_duration_max_us(duration);
        log::info!("[MLME-SCAN] sent beacon frame to channel {channel}, waiting for response for {delay_us}us");

        self.ieee802154.start_receive();
        Delay::new().delay_micros(delay_us);

        // check the rx queue
        let mut scan_result = None;
        while let Some(res) = self.ieee802154.received() {
            match res {
                Ok(frame) => {
                    log::info!(
                        "[MLME-SCAN] receive frame on channel {channel}: {frame:#?}",
                        frame = frame.frame,
                    );

                    scan_result = Some(ScanResult { channel });
                }
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
