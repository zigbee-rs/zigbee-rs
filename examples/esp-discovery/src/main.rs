#![no_std]
#![no_main]

use embedded_storage::ReadStorage;
use esp_alloc as _;
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::FlashRegion;
use esp_hal::main;
use esp_println::println;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Ieee802154;
use esp_storage::FlashStorage;
use zigbee::nwk::nlme::Nlme;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(size: 24 * 1024);

    let mut flash = FlashStorage::new(peripherals.FLASH);
    println!("Flash size = {}", flash.capacity());

    let nlme = Nlme::new(flash);

    let mut ieee802154 = Ieee802154::new(peripherals.IEEE802154);

    ieee802154.set_config(Config {
        channel: 15,
        promiscuous: false,
        rx_when_idle: true,
        auto_ack_rx: true,
        auto_ack_tx: true,
        pan_id: Some(0x4242),
        short_addr: Some(0x2323),
        ..Default::default()
    });

    println!("Start receiving:");
    ieee802154.start_receive();

    loop {
        if let Some(frame) = ieee802154.received() {
            println!("Received {:?}\n", &frame);
        }
    }
}
