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
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::NlmeSap;
use zigbee_mac::esp::EspMlme;

esp_bootloader_esp_idf::esp_app_desc!();

const SCAN_DURATION: u8 = 4u8;
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

    loop {
        println!("Start network discovery, channels: {CHANNELS:?}, duration: {SCAN_DURATION}");
        match nwk.network_discovery(CHANNELS, SCAN_DURATION).await {
            Ok(nd) => {
                println!("Found following Zigbee networks in proximity:");
                println!("{nd:#?}");
            }
            Err(e) => {
                println!("Failed to discovery networks: {e}");
            }
        }

        println!("Sleep for 5 min before starting next scan...");
        Timer::after_secs(300).await;
    }
}
