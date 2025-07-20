#![no_main]
#![no_std]

/* define a default panic handler */
use panic_halt as _;

use cortex_m_rt::entry;
use embedded_hal::digital::InputPin;
use rtt_target::{rprintln, rtt_init_print};
#[cfg(feature = "nrf52833")]
use nrf52833_hal as hal;
#[cfg(feature = "nrf52833")]
use nrf52833_hal::gpio::Level;
#[cfg(feature = "nrf52840")]
use nrf52840_hal as hal;
#[cfg(feature = "nrf52840")]
use nrf52840_hal::gpio::Level;


#[entry]
fn main() -> ! {

    rprintln!("Init zigbee!");
    let zigbee_device = zigbee::init(zigbee::Config { radio_channel: 11, ..Default::default() });

    // init GPIO for device pairing
    let p = hal::pac::Peripherals::take().unwrap();
    let port0 = hal::gpio::p0::Parts::new(p.P0);
    let mut button = port0.p0_11.into_pullup_input();

    loop {
        if button.is_high().unwrap() {
            rprintln!("Connect to nearby zigbee network.");
            zigbee_device.try_to_connect();
        } else if zigbee_device.is_connected() {
            rprintln!("Send keep alive to stay in network.");

            // send keep alive to stay in network
            zigbee_device.send_keep_alive();

            // periodic update of sensor data
            zigbee_device.send_data(&[0x7au8, 0x69u8, 0x67u8, 0x62u8, 0x65u8, 0x65u8]);

        } else {
            rprintln!("Idle.");
            // idle
        }

        cortex_m::asm::delay(1_000);
    }
}

