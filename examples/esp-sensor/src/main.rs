#![no_std]
#![no_main]

use core::ops::Range;

use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_time::Delay;
use embassy_time::Timer;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use esp_storage::FlashStorage;
use static_cell::StaticCell;
use zigbee::CurrentPowerMode;
use zigbee::CurrentPowerSourceLevel;
use zigbee::LogicalType;
use zigbee::PowerSource;
use zigbee::aps::aib;
use zigbee::aps::apsde::ApsdeSapConfirmStatus;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::zdo::config::DiscoveryType;
use zigbee::zdo::descriptor::DeviceDescriptorConfig;
use zigbee::zdo::descriptor::EndpointDescriptor;
use zigbee::zdo::descriptor::NodeDescriptorConfig;
use zigbee::zdo::descriptor::PowerDescriptorConfig;
use zigbee::DeviceConfig;
use zigbee::NetworkConfig;
use zigbee::TimingConfig;
use zigbee::config::StackConfig;
use zigbee_cluster_library::clusters::general::basic;
use zigbee_cluster_library::clusters::general::basic::BasicServer;
use zigbee_cluster_library::clusters::general::identify;
use zigbee_cluster_library::clusters::general::identify::IdentifyServer;
use zigbee_cluster_library::clusters::measurement::temperature;
use zigbee_cluster_library::profile;
use zigbee_cluster_library::reporting::AttributeReportBuilder;
use zigbee_cluster_library::reporting::ConfigureReportingServer;
use zigbee_cluster_library::sender::ZclSender;
use zigbee_cluster_library::sender::ZclReportTarget;
use zigbee_cluster_library::types::integers::Int16;
use zigbee_mac::esp::EspMlme;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

esp_bootloader_esp_idf::esp_app_desc!();

/// flash region reserved for zigbee persistence.
///
/// MUST be adjusted to your partition table: it must not overlap the
/// bootloader, the firmware image, or the esp-idf partition table. it must be
/// erase-sector aligned (4 KiB on esp32-c6) and span at least two sectors so
/// sequential-storage has a spare sector for garbage collection.
const ZIGBEE_FLASH_RANGE: Range<u32> = 0x3f_0000..0x3f_4000;

/// Extended PAN ID of the network to join.
const EXTENDED_PAN_ID: u64 = 0x0000000000000000;

/// Channel to scan on (must match the coordinator's channel).
const CHANNEL: u8 = 11;

/// Scan duration exponent (beacon order).
const SCAN_DURATION: u8 = 5;

/// Application endpoint exposed by this sensor.
const SENSOR_ENDPOINT: u8 = 1;

/// Coordinator-side endpoint to deliver reports to.
const COORDINATOR_ENDPOINT: u8 = 1;

/// allocate address (bit 7), rx-on-when-idle (bit 3) cleared: a polling end
/// device. TI Z-Stack delivers the association response (and all downstream
/// traffic) via indirect transmission extracted by data-request polls; with
/// rx-on-when-idle set it treats the device as always-on and never delivers the
/// association response on the poll path, so the join stalls.
const CAPABILITY: u8 = 0x80;

/// Clusters served on [`SENSOR_ENDPOINT`] (input/server side).
static INPUT_CLUSTERS: [u16; 3] = [
    basic::CLUSTER_ID,
    identify::CLUSTER_ID,
    temperature::CLUSTER_ID,
];
static OUTPUT_CLUSTERS: [u16; 0] = [];

/// Endpoints advertised to the network for service discovery.
static ENDPOINTS: [EndpointDescriptor; 1] = [EndpointDescriptor {
    endpoint: SENSOR_ENDPOINT,
    profile_id: profile::HOME_AUTOMATION,
    // HA device id 0x0302: Temperature Sensor.
    device_id: 0x0302,
    device_version: 1,
    input_clusters: &INPUT_CLUSTERS,
    output_clusters: &OUTPUT_CLUSTERS,
}];

/// Basic cluster identity used by Zigbee2MQTT to resolve the device definition.
static BASIC: BasicServer = BasicServer {
    zcl_version: 8,
    application_version: 1,
    stack_version: 0,
    hw_version: 1,
    manufacturer_name: "zigbee-rs",
    model_identifier: "zigbee-rs.temp-sensor",
    // 0x03: battery.
    power_source: 0x03,
};

type ZigbeeFlash = zigbee::storage::FlashStorage<BlockingAsync<FlashStorage<'static>>>;

/// Cluster servers answering inbound requests, in the order they are tried.
type Handler = (
    BasicServer<'static>,
    ConfigureReportingServer,
    &'static IdentifyServer,
);

/// The running stack this application's tasks share.
type Stack = zigbee::Stack<'static, EspMlme<'static>, Handler, ZigbeeFlash>;

static STACK: StaticCell<Stack> = StaticCell::new();

/// Everything this application configures: the network to be on, what this
/// device is, the cadences, and the descriptors served to an interviewer. The
/// logical type and capability flags of the node descriptor are derived from
/// the device configuration, so they cannot drift apart.
fn stack_config() -> StackConfig<'static> {
    StackConfig::new(
        NetworkConfig {
            extended_pan_id: IeeeAddress(EXTENDED_PAN_ID),
            channels: CHANNEL..CHANNEL + 1,
            scan_duration: SCAN_DURATION,
        },
        DeviceConfig {
            logical_type: LogicalType::EndDevice,
            capability_information: CapabilityInformation(CAPABILITY),
            discovery_type: DiscoveryType::default(),
            tc_link_key_exchange: true,
        },
        TimingConfig {
            poll_interval_ms: POLL_INTERVAL_MS,
            ..TimingConfig::default()
        },
        DeviceDescriptorConfig {
            node: NodeDescriptorConfig {
                // logical_type and mac_capability_flags come from DeviceConfig
                // bit 3: 2400 MHz band.
                frequency_band: 0x08,
                manufacturer_code: 0x1037,
                maximum_buffer_size: 80,
                maximum_incoming_transfer_size: 128,
                server_mask: 0,
                maximum_outgoing_transfer_size: 128,
                descriptor_capability_field: 0,
                ..NodeDescriptorConfig::default()
            },
            power: PowerDescriptorConfig {
                current_power_mode: CurrentPowerMode::Stimulated,
                available_power_sources: &[PowerSource::DisposableBattery],
                current_power_source: PowerSource::DisposableBattery,
                current_power_source_level: CurrentPowerSourceLevel::Full,
            },
            endpoints: &ENDPOINTS,
        },
    )
}

/// Parent poll interval; must stay below the parent's ~7.68 s
/// indirect-transaction persistence time.
const POLL_INTERVAL_MS: u32 = 500;

/// Identify state, shared between the receive task and the application.
static IDENTIFY: IdentifyServer = IdentifyServer::new();

/// Drives the stack: receives and answers ZDP discovery, Basic-cluster and
/// Identify reads, Identify commands, Configure Reporting requests and APS
/// commands, keeps the parent link alive, and persists NIB/AIB changes.
#[embassy_executor::task]
async fn stack_task(stack: &'static Stack) {
    // returns only when no rejoin could get us back onto the network: the
    // network is gone or our keys are stale, so re-commission from scratch
    let outcome = stack.run(Delay).await;
    println!("Off the network ({outcome:?}), forgetting it and rebooting");
    stack.forget_network().await;
    esp_hal::system::software_reset();
}

/// Counts the Identify state down once a second (ZCL 3.5.2.2.1); a real device
/// would blink an LED here.
#[embassy_executor::task]
async fn identify_task() {
    loop {
        Timer::after_secs(1).await;
        if IDENTIFY.is_identifying() {
            println!("Identifying ({}s remaining)", IDENTIFY.tick(1));
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    esp_alloc::heap_allocator!(size: 24 * 1024);

    // the stack owns persistence: the information bases are restored here and
    // the stack persists dirty state (keys, frame counters, tables) whenever it
    // changes them
    let flash = BlockingAsync::new(FlashStorage::new(peripherals.FLASH));
    let storage = zigbee::storage::init_with_flash(flash, ZIGBEE_FLASH_RANGE).await;

    let config = stack_config();

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    // the stack programs the radio from the restored information base: on a
    // reboot while joined it resumes instead of re-commissioning
    let mac = EspMlme::new(ieee802154, esp_radio::ieee802154::Config::default());
    println!("Device IEEE address: {:#018x}", mac.ieee_address());

    let handler = (BASIC, ConfigureReportingServer, &IDENTIFY);
    let stack: &'static Stack = STACK.init(Stack::new(mac, config, handler, storage));

    // the stack task is all it takes: it commissions the device, answers the
    // interview, keeps the parent link alive and persists NIB/AIB changes
    spawner.spawn(stack_task(stack).expect("spawn stack_task"));
    spawner.spawn(identify_task().expect("spawn identify_task"));

    stack.wait_until_joined().await;

    let nib = zigbee::nwk::nib::get_ref();
    println!(
        "On network: addr={:#06x} pan={:#06x} epid={:#x} channel={} update_id={}",
        *nib.network_address(),
        *nib.panid(),
        *nib.extended_panid(),
        stack.config().channel(),
        nib.update_id()
    );
    if let Some(material) = nib.security_material_set().first() {
        println!("Network key installed: key={:02x?}", material.key);
    }
    if let Some(pair) = aib::get_ref().device_key_pair_set().first() {
        println!("Link key installed: key={:02x?}", pair.link_key);
    }

    let mut zcl_seq: u8 = 0;
    let mut sample: i16 = 2300; // 23.00 °C in hundredths
    loop {
        zcl_seq = zcl_seq.wrapping_add(1);

        let mut buf = [0u8; 64];
        let report = AttributeReportBuilder::new(&mut buf, zcl_seq)
            .and_then(|frame| frame.push(&temperature::MEASURED_VALUE, Int16(sample)))
            .and_then(AttributeReportBuilder::finish)
            .expect("encode temperature report");

        let result = stack
            .device()
            .send_attribute_report(
                ZclReportTarget {
                    dst_short: ShortAddress::COORDINATOR.0,
                    src_endpoint: SENSOR_ENDPOINT,
                    dst_endpoint: COORDINATOR_ENDPOINT,
                    profile_id: profile::HOME_AUTOMATION,
                },
                report,
                &mut Delay,
            )
            .await;

        match result {
            Ok(confirm) if confirm.status == ApsdeSapConfirmStatus::Success => {
                println!("Reported temperature: {} (seq={})", sample, zcl_seq);
            }
            // a lost parent link is detected by the stack and handled by the
            // link task; anything reported here is local or transient
            Ok(confirm) => println!("Report failed: {:?}", confirm.status),
            Err(e) => println!("Encode error: {:?}", e),
        }

        sample = sample.wrapping_add(10);
        Timer::after_secs(30).await;
    }
}
