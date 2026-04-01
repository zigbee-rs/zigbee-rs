#![no_std]
#![no_main]

use embassy_time::Timer;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee_base_device_behavior::BaseDeviceBehavior;
use zigbee_mac::esp::EspMlme;
use zigbee_types::IeeeAddress;

esp_bootloader_esp_idf::esp_app_desc!();

/// Extended PAN ID of the network to join.
const EXTENDED_PAN_ID: u64 = 0xf4ce36c17d3852e1;

/// Channel to scan on (must match the coordinator's channel).
const CHANNEL: u8 = 16;

/// Scan duration exponent (beacon order).
const SCAN_DURATION: u8 = 5;

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    esp_alloc::heap_allocator!(size: 24 * 1024);

    zigbee::nwk::nib::init(zigbee::nwk::nib::NibStorage::default());
    zigbee::aps::aib::init(zigbee::aps::aib::AibStorage::default());

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    let mac = EspMlme::new(ieee802154, Default::default());
    let nwk = Nlme::new(mac);

    let config = zigbee::Config {
        device_type: zigbee::LogicalType::EndDevice,
        ..zigbee::Config::default()
    };
    let mut bdb = BaseDeviceBehavior::new(nwk, config);

    println!("Joining EPID={EXTENDED_PAN_ID:#018x} on channel {CHANNEL}...");
    match bdb
        .network_steering(
            IeeeAddress(EXTENDED_PAN_ID),
            CHANNEL..CHANNEL + 1,
            SCAN_DURATION,
            CapabilityInformation(0x80),
        )
        .await
    {
        Ok(confirm) if confirm.status == NlmeJoinStatus::Success => {
            let nib = bdb.nib();
            println!(
                "Joined: addr={:#06x} pan={:#06x} epid={:#x} update_id={}",
                nib.network_address(),
                nib.panid(),
                nib.extended_panid(),
                nib.update_id()
            );

            let sec = nib.security_material_set();
            if let Some(key) = sec.first() {
                println!(
                    "Network key installed: seq={} key={:02x?}",
                    key.key_seq_number, key.key.0
                );
            }
        }
        Ok(confirm) => {
            println!("Join failed: {:?}", confirm.status);
        }
        Err(e) => {
            println!("Join error: {e}");
        }
    }

    loop {
        Timer::after_secs(60).await;
    }
}
