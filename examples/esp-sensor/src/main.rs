#![no_std]
#![no_main]

use embassy_time::Timer;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use static_cell::StaticCell;
use zigbee::LogicalType;
use zigbee::aps::aib;
use zigbee::aps::apsde::ApsdeSapConfirmStatus;
use zigbee::nwk::nib::CapabilityInformation;
use zigbee::nwk::nlme::Nlme;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
use zigbee::zdo::ZigbeeDevice;
use zigbee::zdo::descriptor::DeviceDescriptorConfig;
use zigbee::zdo::descriptor::EndpointDescriptor;
use zigbee::zdo::descriptor::NodeDescriptorConfig;
use zigbee_base_device_behavior::BaseDeviceBehavior;
use zigbee_cluster_library::basic;
use zigbee_cluster_library::basic::BasicServer;
use zigbee_cluster_library::common::data_types::SignedN;
use zigbee_cluster_library::common::data_types::ZclDataType;
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
static INPUT_CLUSTERS: [u16; 2] = [basic::CLUSTER_ID, temperature::CLUSTER_ID];
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

static DEVICE: StaticCell<ZigbeeDevice<EspMlme<'static>>> = StaticCell::new();

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
        endpoints: &ENDPOINTS,
    }
}

/// Steady-state receive loop: answers ZDP discovery, Basic-cluster reads, and
/// Configure Reporting requests.
#[embassy_executor::task]
async fn rx_task(device: &'static ZigbeeDevice<EspMlme<'static>>) {
    device
        .rx_loop(&descriptor_config(), &(BASIC, ConfigureReportingServer))
        .await
}

#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
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
    println!("Device IEEE address: {:#018x}", mac.ieee_address());
    let nlme = Nlme::new(mac);

    let config = zigbee::Config {
        device_type: LogicalType::EndDevice,
        ..zigbee::Config::default()
    };
    let device: &'static ZigbeeDevice<EspMlme<'static>> =
        DEVICE.init(ZigbeeDevice::new(config, nlme));
    let mut bdb = BaseDeviceBehavior::new(config);

    println!("Joining EPID={EXTENDED_PAN_ID:#018x} on channel {CHANNEL}...");
    let join = bdb
        .network_steering(
            device,
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
                nib.network_address(),
                nib.panid(),
                nib.extended_panid(),
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

    // Spawn the receive loop so the interview (Node_Desc / Active_EP /
    // Simple_Desc / Basic reads) is answered while the report loop runs.
    spawner.spawn(rx_task(device).expect("spawn rx_task"));

    let mut zcl_seq: u8 = 0;
    let mut sample: i16 = 2300; // 23.00 °C in hundredths
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
            )
            .await;

        match result {
            Ok(confirm) if confirm.status == ApsdeSapConfirmStatus::Success => {
                println!("Reported temperature: {} (seq={})", sample, zcl_seq);
            }
            Ok(confirm) => println!("Report failed: {:?}", confirm.status),
            Err(e) => println!("Encode error: {:?}", e),
        }

        sample = sample.wrapping_add(10);
        Timer::after_secs(30).await;
    }
}
