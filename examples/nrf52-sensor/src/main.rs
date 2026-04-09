#![no_main]
#![no_std]

// TODO: this example requires an nrf52 `Mlme` implementation in zigbee-mac.
// Once available, wire it up similarly to examples/esp-sensor.

use panic_halt as _;

// required to provide the interrupt vector table via cortex-m-rt
#[cfg(feature = "nrf52833")]
use nrf52833_hal as _;
#[cfg(feature = "nrf52840")]
use nrf52840_hal as _;

use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    loop {}
}
