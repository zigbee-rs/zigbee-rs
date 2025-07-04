#![no_std]
#![no_main]

use esp_backtrace as _;

use esp_hal::gpio::{Input, Pull};
use esp_hal::prelude::*;
use esp_hal::delay::Delay;
use esp_println::println;

#[entry]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let delay = Delay::new();

    log::info!("Init zigbee!");
    let zigbee_device = zigbee::init(zigbee::Config { radio_channel: 11, ..Default::default() });

    // init GPIO for device pairing
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let button = Input::new(peripherals.GPIO9, Pull::Down);
    loop {
        if button.is_low() {
            println!("Connect to nearby zigbee network.");
            zigbee_device.try_to_connect();
        } else if zigbee_device.is_connected() {
            println!("Send keep alive to stay in network.");

            // send keep alive to stay in network
            zigbee_device.send_keep_alive();

            // periodic update of sensor data
            zigbee_device.send_data(&[0x7au8, 0x69u8, 0x67u8, 0x62u8, 0x65u8, 0x65u8]);

        } else {
            println!("Idle.");
            // idle
        }

        delay.delay(1_000.millis());
    }
}

