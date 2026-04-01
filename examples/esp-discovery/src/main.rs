#![no_std]
#![no_main]

use embassy_time::Timer;
use embedded_storage::ReadStorage;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use esp_storage::FlashStorage;
use heapless::Vec;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::NlmeSap;
use zigbee::nwk::nlme::management::NetworkDescriptor;
use zigbee::nwk::nlme::management::NlmeJoinRequest;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee_mac::esp::EspMlme;

esp_bootloader_esp_idf::esp_app_desc!();

const SCAN_DURATION: u8 = 10u8;
const CHANNELS: core::ops::Range<u8> = 11..27;

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    esp_alloc::heap_allocator!(size: 24 * 1024);

    let flash = FlashStorage::new(peripherals.FLASH);
    println!("Flash size = {}", flash.capacity());

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    let mac = EspMlme::new(ieee802154, Default::default());

    let mut nwk = Nlme::new(flash, mac);
    nwk.nib.init();

    println!("Starting network discovery, channels: {CHANNELS:?}, duration: {SCAN_DURATION}");
    match nwk.network_discovery(CHANNELS, SCAN_DURATION).await {
        Ok(nd) => {
            let mut visited: Vec<zigbee_types::ShortAddress, 10> = heapless::Vec::new();
            let nd: Vec<NetworkDescriptor, 10> = nd
                .network_descriptor
                .into_iter()
                .filter(|nd| {
                    if visited.contains(&nd.pan_id) {
                        false
                    } else {
                        let _ = visited.push(nd.pan_id);
                        true
                    }
                })
                .collect();

            println!("Found following Zigbee networks in proximity:");
            println!("{nd:#?}");

            println!("Neighbor Table: ");
            let nt = nwk.nib.neighbor_table();
            println!("{nt:#?}");

            let network = nd
                .iter()
                .find(|nd| nd.permit_joining)
                .expect("no open network (permitJoin = true) found");

            let request = NlmeJoinRequest {
                extended_pan_id: network.extended_pan_id,
                rejoin_network: 0x00,
                scan_duration: 0x00,
                capability_information: CapabilityInformation(0x80),
                security_enabled: false,
            };

            println!("Joining network EPID={:?}...", request.extended_pan_id);
            let confirm = nwk.join(request).await;
            println!("Join result: {confirm:#?}");

            if confirm.status == NlmeJoinStatus::Success {
                println!(
                    "NIB: addr={:#06x} pan={:#06x} epid={:#x} update_id={}",
                    nwk.nib.network_address(),
                    nwk.nib.panid(),
                    nwk.nib.extended_panid(),
                    nwk.nib.update_id()
                );

                // Wait for the Trust Center to deliver the network key
                println!("Waiting for transport key from Trust Center...");
                match zigbee::aps::security::await_transport_key(&mut nwk).await {
                    Ok(()) => {
                        let sec = nwk.nib.security_material_set();
                        if let Some(key) = sec.first() {
                            println!(
                                "Network key installed: seq={} key={:02x?}",
                                key.key_seq_number, key.key.0
                            );
                        }
                    }
                    Err(e) => println!("Transport key failed: {e}"),
                }
            }
        }
        Err(e) => {
            println!("Discovery failed: {e}");
        }
    }

    loop {
        Timer::after_secs(60).await;
    }
}
