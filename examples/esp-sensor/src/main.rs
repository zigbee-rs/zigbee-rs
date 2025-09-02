#![no_std]
#![no_main]
#![allow(unused_imports, unused_crate_dependencies)]
use core::cell::RefCell;
use core::panic::PanicInfo;

use critical_section::Mutex;
use embedded_storage::Storage;
use esp32c6::Peripherals;
use esp32c6::TIMG0;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::Event;
use esp_hal::gpio::Input;
use esp_hal::gpio::InputConfig;
use esp_hal::gpio::Io;
use esp_hal::gpio::Pull;
use esp_hal::handler;
use esp_hal::main;
use esp_hal::riscv;
use esp_hal::time::Duration;
use esp_hal::time::Instant;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::timer::PeriodicTimer;
use esp_hal::timer::Timer;
use esp_hal::Blocking;
use esp_println::logger::init_logger;
use esp_println::println;
use esp_storage::FlashStorage;
use esp_storage::FlashStorageError;
use log::info;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::NlmeSap;
use zigbee::LogicalType;
use zigbee_base_device_behavior::BaseDeviceBehavior;

static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static TIMER:  Mutex<RefCell<Option<PeriodicTimer<'_, Blocking>>>> = Mutex::new(RefCell::new(None));

#[main]
fn main() -> ! {
    init_logger(log::LevelFilter::Trace);
    log::error!("application start!");

    let mut storage = FlashStorage::new();
    let offset = 0;
    // bdbIsNodeOnANetwork = true
    let data: &[u8] = &[1];
    storage.write(offset, data).expect("Failed to write to storage");

    let nlme = Nlme {};

    let config = zigbee::Config {
        device_type: LogicalType::EndDevice,
        ..zigbee::Config::default()
    };

    let bdb_commisioning_capability = 0u8;

    let mut bdb = BaseDeviceBehavior::new(storage, &nlme, config, bdb_commisioning_capability);
    let _ = bdb.start_initialization_procedure();

    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut button = Input::new(peripherals.GPIO9, InputConfig::default());

    // setup button interrupt for pairing and force update
    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(button_interrupt_handler);
    critical_section::with(|cs| {
        button.listen(Event::FallingEdge);
        BUTTON.borrow_ref_mut(cs).replace(button)
    });

    // setup timer interrupt for regular updates
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut periodic = PeriodicTimer::new(timg0.timer0);
    periodic.set_interrupt_handler(timer_interrupt_handler);
    periodic.enable_interrupt(true);
    let _ = periodic.start(Duration::from_secs(30));
    critical_section::with(|cs| {
        TIMER.borrow_ref_mut(cs).replace(periodic);
    });

    loop {
        // Wait for half a second
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
    }
}

#[handler]
fn button_interrupt_handler() {
    critical_section::with(|cs| {
        log::error!("Button interrupt!");
        if let Some(button) = BUTTON.borrow_ref_mut(cs).as_mut() {
            button.clear_interrupt();
        };
    });
}

#[handler]
fn timer_interrupt_handler() {
    critical_section::with(|cs| {
        if let Some(timer) = TIMER.borrow_ref_mut(cs).as_mut() {
            timer.clear_interrupt();
            log::error!("Timer interrupt!");
        };
    });
}

