#![no_std]
#![no_main]

use embedded_storage::ReadStorage;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::main;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use esp_storage::FlashStorage;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::NlmeSap;
use zigbee_mac::esp::EspMlme;

esp_bootloader_esp_idf::esp_app_desc!();

const SCAN_DURATION: u8 = 4u8;
const CHANNELS: core::ops::Range<u8> = 11..27;

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(size: 24 * 1024);
    let delay = Delay::new();

    let flash = FlashStorage::new(peripherals.FLASH);
    println!("Flash size = {}", flash.capacity());

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    let mac = EspMlme::new(ieee802154);

    let mut nwk = Nlme::new(flash, mac);

    loop {
        println!("Start network discovery, channels: {CHANNELS:?}, duration: {SCAN_DURATION}");
        match nwk.network_discovery(CHANNELS, SCAN_DURATION) {
            Ok(nd) => {
                println!("Found following Zigbee networks in proximity:");
                println!("{nd:#?}");
            }
            Err(e) => {
                println!("Failed to discovery networks: {e}");
            }
        }

        println!("Sleep for 5 min before starting next scan...");
        delay.delay_millis(300_000);
    }
}
