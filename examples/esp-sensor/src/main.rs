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
use zigbee::LogicalType;
use zigbee::aps::aib;
use zigbee::aps::apsde::ApsdeSapConfirmStatus;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee::storage::StorageDriver;
use zigbee::zdo::ZigbeeDevice;
use zigbee::zdo::descriptor::DeviceDescriptorConfig;
use zigbee::zdo::descriptor::EndpointDescriptor;
use zigbee::zdo::descriptor::NodeDescriptorConfig;
use zigbee::zdo::descriptor::PowerDescriptorConfig;
use zigbee_base_device_behavior::BaseDeviceBehavior;
use zigbee_cluster_library::basic;
use zigbee_cluster_library::basic::BasicServer;
use zigbee_cluster_library::common::data_types::SignedN;
use zigbee_cluster_library::common::data_types::ZclDataType;
use zigbee_cluster_library::identify;
use zigbee_cluster_library::identify::IdentifyServer;
use zigbee_cluster_library::measurement::temperature;
use zigbee_cluster_library::profile;
use zigbee_cluster_library::reporting::ConfigureReportingServer;
use zigbee_cluster_library::sender::ZclSender;
use zigbee_cluster_library::sender::ZclUnicast;
use zigbee_cluster_library::sender::build_report_attributes;
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

static DEVICE: StaticCell<ZigbeeDevice<EspMlme<'static>>> = StaticCell::new();
static STORAGE: StaticCell<ZigbeeFlash> = StaticCell::new();

fn descriptor_config() -> DeviceDescriptorConfig<'static> {
    DeviceDescriptorConfig {
        node: NodeDescriptorConfig {
            logical_type: LogicalType::EndDevice,
            complex_descriptor_available: false,
            user_descriptor_available: false,
            // bit 3: 2400 MHz band.
            frequency_band: 0x08,
            mac_capability_flags: CAPABILITY,
            manufacturer_code: 0x1037,
            maximum_buffer_size: 80,
            maximum_incoming_transfer_size: 128,
            server_mask: 0,
            maximum_outgoing_transfer_size: 128,
            descriptor_capability_field: 0,
        },
        // disposable battery (bit 2), on when stimulated, full charge
        power: PowerDescriptorConfig {
            current_power_mode: 0b0010,
            available_power_sources: 0b0100,
            current_power_source: 0b0100,
            current_power_source_level: 0b1100,
        },
        endpoints: &ENDPOINTS,
    }
}

/// Parent poll interval; must stay below the parent's ~7.68 s
/// indirect-transaction persistence time.
const POLL_INTERVAL_MS: u32 = 500;

/// Identify state, shared between the receive task and the application.
static IDENTIFY: IdentifyServer = IdentifyServer::new();

/// Receive loop: idles until the join completes, then answers ZDP discovery,
/// Basic-cluster and Identify reads, Identify commands, Configure Reporting
/// requests, and APS commands (TC link key exchange).
#[embassy_executor::task]
async fn rx_task(device: &'static ZigbeeDevice<EspMlme<'static>>) {
    let cfg = descriptor_config();
    let handler = (BASIC, ConfigureReportingServer, &IDENTIFY);
    device
        .rx_loop(&cfg, &handler, &mut Delay, POLL_INTERVAL_MS)
        .await
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

    // the stack owns persistence: it restores the information bases here and
    // the storage task persists dirty state (keys, frame counters, tables)
    // whenever the stack changes it
    let flash = BlockingAsync::new(FlashStorage::new(peripherals.FLASH));
    let storage: &'static ZigbeeFlash =
        STORAGE.init(zigbee::storage::init_with_flash(flash, ZIGBEE_FLASH_RANGE).await);

    // a restored short address means we are still on the network: the parent
    // keeps sleepy children across our reboot, so resume polling instead of
    // re-commissioning (NLME-JOIN refuses to re-associate a joined device)
    let nib = zigbee::nwk::nib::get_ref();
    let resume = *nib.network_address() != 0xffff;

    // on resume the radio must be configured from the restored NIB up front —
    // association normally does this, but we skip it
    let mut mac_config = esp_radio::ieee802154::Config::default();
    if resume {
        mac_config.channel = CHANNEL;
        mac_config.pan_id = Some(*nib.panid());
        mac_config.short_addr = Some(*nib.network_address());
        mac_config.auto_ack_tx = true;
        mac_config.auto_ack_rx = true;
    }

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    let mac = EspMlme::new(ieee802154, mac_config);
    println!("Device IEEE address: {:#018x}", mac.ieee_address());
    let nlme = Nlme::new(mac);

    let config = zigbee::Config {
        device_type: LogicalType::EndDevice,
        ..zigbee::Config::default()
    };
    let device: &'static ZigbeeDevice<EspMlme<'static>> =
        DEVICE.init(ZigbeeDevice::new(config, nlme));
    let mut bdb = BaseDeviceBehavior::new(config);

    // spawn the receive loop up front; it idles until the join completes,
    // then answers the interview and delivers the TC link key replies
    spawner.spawn(rx_task(device).expect("spawn rx_task"));
    // persist NIB/AIB changes as they happen, including the keys obtained
    // during commissioning
    spawner.spawn(storage_task(storage).expect("spawn storage_task"));
    spawner.spawn(identify_task().expect("spawn identify_task"));

    if resume {
        println!(
            "Resuming on network: addr={:#06x} pan={:#06x} channel={CHANNEL}, attempting rejoin...",
            *nib.network_address(),
            *nib.panid(),
        );
    }

    match bdb.start_initialization_procedure(device, &mut Delay).await {
        Ok(Some(confirm)) if confirm.status == NlmeJoinStatus::Success => {
            println!(
                "Rejoined: addr={:#06x} pan={:#06x} channel={CHANNEL}",
                *nib.network_address(),
                *nib.panid(),
            );
        }
        init_result => {
            // a prior rejoin attempt leaves the old network address/key
            // material in the NIB; forget it before commissioning fresh so
            // NLME-JOIN's "already joined" check does not reject it
            if let Ok(Some(confirm)) = init_result {
                println!("Rejoin failed ({:?}), forgetting network", confirm.status);
                device.forget_network();
            }

            println!("Joining EPID={EXTENDED_PAN_ID:#018x} on channel {CHANNEL}...");
            let join = bdb
                .network_steering(
                    device,
                    &mut Delay,
                    IeeeAddress(EXTENDED_PAN_ID),
                    CHANNEL..CHANNEL + 1,
                    SCAN_DURATION,
                    CapabilityInformation(CAPABILITY),
                )
                .await;

            match join {
                Ok(confirm) if confirm.status == NlmeJoinStatus::Success => {
                    let nib = bdb.nib();
                    println!(
                        "Joined: addr={:#06x} pan={:#06x} epid={:#x} update_id={}",
                        *nib.network_address(),
                        *nib.panid(),
                        *nib.extended_panid(),
                        nib.update_id()
                    );

                    let network_key = nib.security_material_set().first().unwrap().key;
                    println!("Network key installed: key={:02x?}", network_key);

                    let link_key = aib::get_ref()
                        .device_key_pair_set()
                        .first()
                        .unwrap()
                        .link_key;
                    println!("Link key installed: key={:02x?}", link_key);
                }
                Ok(confirm) => {
                    println!("Join failed: {:?}", confirm.status);
                    loop {
                        Timer::after_secs(60).await;
                    }
                }
                Err(e) => {
                    println!("Join error: {e:#}");
                    loop {
                        Timer::after_secs(60).await;
                    }
                }
            }
        }
    }

    let mut zcl_seq: u8 = 0;
    let mut sample: i16 = 2300; // 23.00 °C in hundredths
    let mut report_failures: u32 = 0;
    loop {
        zcl_seq = zcl_seq.wrapping_add(1);

        let frame = build_report_attributes(
            zcl_seq,
            [(
                temperature::attribute::MEASURED_VALUE,
                ZclDataType::SignedInt(SignedN::Int16(sample)),
            )],
        )
        .expect("encode temperature report");

        let result = device
            .send_zcl_unicast(
                ZclUnicast {
                    dst_short: ShortAddress::COORDINATOR.0,
                    src_endpoint: SENSOR_ENDPOINT,
                    dst_endpoint: COORDINATOR_ENDPOINT,
                    profile_id: profile::HOME_AUTOMATION,
                    cluster_id: temperature::CLUSTER_ID,
                },
                frame,
                &mut Delay,
            )
            .await;

        match result {
            Ok(confirm) if confirm.status == ApsdeSapConfirmStatus::Success => {
                println!("Reported temperature: {} (seq={})", sample, zcl_seq);
                report_failures = 0;
            }
            // only a missing acknowledgement points at an unreachable parent;
            // the other statuses are local or transient
            Ok(confirm) => {
                println!("Report failed: {:?}", confirm.status);
                if confirm.status == ApsdeSapConfirmStatus::NoAck {
                    report_failures += 1;
                }
            }
            Err(e) => println!("Encode error: {:?}", e),
        }

        // an unreachable parent (no MAC ack) means we were likely aged out of
        // its child table — forget the network and re-commission from scratch
        if report_failures >= MAX_REPORT_FAILURES {
            println!("Parent unreachable, forgetting network and rebooting");
            device.forget_network();
            // make sure the cleared state hits flash before the reset
            storage.flush().await;
            esp_hal::system::software_reset();
        }

        sample = sample.wrapping_add(10);
        Timer::after_secs(30).await;
    }
}

/// consecutive report failures before the network is forgotten and the device
/// re-commissions.
const MAX_REPORT_FAILURES: u32 = 4;

/// Persists information-base changes (keys, frame counters, neighbor table)
/// to flash as the stack makes them.
#[embassy_executor::task]
async fn storage_task(storage: &'static ZigbeeFlash) {
    storage.run().await
}
