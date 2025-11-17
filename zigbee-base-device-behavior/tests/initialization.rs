#![allow(unused_variables)]

pub const STORAGE_SIZE: usize = 15;

use embedded_storage::Storage;
use zigbee::nwk::nlme::management::NlmeJoinConfirm;
use zigbee::nwk::nlme::management::NlmeJoinStatus;
#[cfg(feature = "mock")]
use zigbee::nwk::nlme::MockNlmeSap;
use zigbee::LogicalType;
use zigbee_base_device_behavior::BaseDeviceBehavior;
use zigbee_types::storage::InMemoryStorage;

#[test]
fn enddevice_on_network_should_trigger_rejoin() {
    // given
    let mut storage = InMemoryStorage::<STORAGE_SIZE>::default();
    let offset = 0;
    let data: &[u8] = &[1];
    storage
        .write(offset, data)
        .expect("Failed to write to storage");

    let mut nlme = MockNlmeSap::new();

    let join_confirm = NlmeJoinConfirm {
        status: NlmeJoinStatus::Success,
        network_address: 1337u16,
        extended_pan_id: 12345u64,
        enhanced_beacon_type: false,
        mac_interface_index: 0u8,
    };
    nlme.expect_rejoin().return_once(|| join_confirm);
    nlme.expect_join().times(0);
    nlme.expect_network_formation().times(0);
    //nlme.expect_network_discovery().times(0);
    let config = zigbee::Config {
        device_type: LogicalType::EndDevice,
        ..zigbee::Config::default()
    };

    let bdb_commisioning_capability = 0u8;
    let mut bdb = BaseDeviceBehavior::new(storage, &nlme, config, bdb_commisioning_capability);

    // when
    let result = bdb.start_initialization_procedure();

    // then
    assert!(result.is_ok());
}
