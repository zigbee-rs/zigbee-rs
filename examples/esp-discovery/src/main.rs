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
use zigbee_mac::Mlme;
use zigbee_mac::ScanType;
use zigbee_mac::esp::EspMlme;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(size: 24 * 1024);

    let flash = FlashStorage::new(peripherals.FLASH);
    println!("Flash size = {}", flash.capacity());

    let _nlme = Nlme::new(flash);

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    let mut mlme = EspMlme::new(ieee802154);

    loop {
        println!("start scanning for networks");
        mlme.scan_network(ScanType::Active, 0xffff_ffff, 20);
        Delay::new().delay_millis(5_000);
    }
}
