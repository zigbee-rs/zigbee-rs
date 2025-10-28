use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Frame;
use esp_radio::ieee802154::Ieee802154;
use ieee802154::mac::beacon::Beacon;
use ieee802154::mac::beacon::BeaconOrder;
use ieee802154::mac::beacon::GuaranteedTimeSlotInformation;
use ieee802154::mac::beacon::PendingAddress;
use ieee802154::mac::beacon::SuperframeOrder;
use ieee802154::mac::beacon::SuperframeSpecification;
use ieee802154::mac::FrameContent;
use ieee802154::mac::Header;

use crate::Mlme;
use crate::MAX_IEEE802154_CANNELS;

pub struct EspMlme {
    ieee802154: Ieee802154,
}

impl Mlme for EspMlme {
    fn scan_network(&self, ty: crate::ScanType, channels: u32, duration: u8) {
        for channel in (0..MAX_IEEE802154_CANNELS) {
            if channels << channel & 0b1 {
                let config = Config {
                    channel,
                    promiscuous: true,
                    rx_when_idle: true,
                    auto_ack_rx: true,
                    auto_ack_tx: true,
                    ..Default::default()
                };
                self.ieee802154.set_config(cfg);

                Frame {
                    header: Header {
                        ..Default::default()
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
                    payload: None,
                    footer: None,
                };
                self.ieee802154.transmit(frame);
            }
        }
        todo!()
    }
}
