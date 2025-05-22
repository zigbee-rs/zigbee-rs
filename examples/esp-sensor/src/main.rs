#![allow(unused_imports)]
#![allow(dead_code)]

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::Event;
use esp_hal::gpio::Input;
use esp_hal::gpio::Io;
use esp_hal::gpio::Level;
use esp_hal::gpio::Output;
use esp_hal::gpio::Pull;
use esp_hal::handler;
use esp_hal::interrupt;
use esp_hal::main;

use esp_hal::interrupt::InterruptConfigurable;
// use esp_hal::riscv::interrupt;
// use esp_hal::interrupt;
// use esp_hal::riscv::interrupt;

use esp_hal::ram;
use esp_hal::riscv;
use esp_hal::time::ExtU64;
use esp_hal::timer::timg::TimerGroup;
// use esp_hal::main;
// use esp_hal::ram;
// use esp32_hal::{
//     clock::ClockControl,
//     pac::Peripherals,
//     prelude::*,
//     timer::{TimerGroup, Timer0},
// };
// use esp_hal::prelude::*;
// use esp_hal::delay::Delay;
use esp_println::println;
use esp_hal::timer::PeriodicTimer;
// use zigbee_cluster_library::measurement::temperature::TemperatureMeasurement;
// use zigbee_cluster_library::ZclFrame;

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    log::info!("Init zigbee!");
    // let zigbee_device = zigbee::init(zigbee::Config { radio_channel: 11,
    // ..Default::default() });

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    // let peripherals = esp_hal::init(esp_hal::Config::default());

    // init GPIO for device pairing
    let button = peripherals.GPIO9;
    let button = Input::new(button, Pull::Up);

    // clock for timer interrupr

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut periodic = PeriodicTimer::new(timg0.timer0);
    let _ = periodic.start(15u64.secs());
    periodic.enable_interrupt(true);
    // let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    // let mut timer_group = TimerGroup::new(peripherals.TIMG0, &clocks);
    // let mut timer0 = timer_group.timer0;
    // timer0.start(1u64.secs()); // Set timer to trigger every 1 second
    // timer0.enable_interrupt();

    // setup interrupr function
    unsafe {
        interrupt::enable(TG0_T0_LEVEL, interrupt::Priority::Priority1);
        // esp32_hal::interrupt::enable(esp32_hal::interrupt::TG0_T0_LEVEL,
            // esp32_hal::interrupt::Priority::Priority1).unwrap(); 
    }

    let delay = Delay::new();
    loop {
        if button.is_low() {
            println!("Connect to nearby zigbee network.");
        //     zigbee_device.try_to_connect();
        // } else if zigbee_device.is_connected() {
        //     println!("Send keep alive to stay in network.");
        //
        //     // send keep alive to stay in network
        //     zigbee_device.send_keep_alive();
        //
        //     // TODO: read latest temperature values
        //
        // let temperature_measurement = TemperatureMeasurement::Measured(13.4);
        //     zigbee_device.send_data(temperature_measurement.to_bytes().as_slice());
        } else {
            println!("Idle.");
            // idle
        }

        delay.delay_millis(1_000);
    }
}

// #[interrupt] // 💥 cannot find attribute `interrupt` in this scope 
#[handler]
#[ram]
/// interrupt handler for TimerGroup 0, Timer 0
fn TG0_T0_LEVEL() {
    println!("interrupt called!");
}

