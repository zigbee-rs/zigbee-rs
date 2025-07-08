#![no_std]
#![no_main]

use esp_backtrace as _;

use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::main;
use esp_hal::time::Duration;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::timer::{PeriodicTimer, Timer};
use esp_println::logger::init_logger;

#[main]
fn main() -> ! {
    init_logger(log::LevelFilter::Debug);
    log::error!("Init zigbee!");

    // init GPIO for device pairing
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut periodic = PeriodicTimer::new(timg0.timer0);

    // Configure the timer for a 30-second interval
    let _ = periodic.start(Duration::from_secs(30));

    let zigbee_device = zigbee::init(zigbee::Config { radio_channel: 11, ..Default::default() });


    let config = InputConfig::default().with_pull(Pull::Down);
    let button = Input::new(peripherals.GPIO9, config);

    loop {
        if button.is_low() {
            log::info!("Connect to nearby zigbee network.");
            zigbee_device.try_to_connect();
        } else if zigbee_device.is_connected() {
            periodic.wait();
            log::info!("Send keep alive to stay in network.");

            // send keep alive to stay in network
            zigbee_device.send_keep_alive();

            // periodic update of sensor data
            zigbee_device.send_data(&[0x7au8, 0x69u8, 0x67u8, 0x62u8, 0x65u8, 0x65u8]);

        } else {
            log::debug!("Idle.");
            // idle
        }
    }
}

