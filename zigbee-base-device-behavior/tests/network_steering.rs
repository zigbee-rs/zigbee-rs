#![allow(unused_variables)]

use embedded_storage::ReadStorage;
use embedded_storage::Storage;
use zigbee::nwk::nlme::management::NetworkDescriptor;
use zigbee::nwk::nlme::management::NlmeNetworkDiscoveryConfirm;
use zigbee::nwk::nlme::management::NlmeNetworkDiscoveryRequest;
use zigbee::nwk::nlme::management::NlmeNetworkDiscoveryStatus::Successful;
use zigbee::nwk::nlme::NlmeSap;
use zigbee_base_device_behavior::types::NetworkSteeringEvent;
use zigbee_base_device_behavior::BaseDeviceBehavior;

#[derive(Default)]
pub struct MockNlme<F>
    where
        F: FnMut(NetworkSteeringEvent)
    {
    pub callback: F,
    pub network_discovery_called: bool,
    pub network_formation_called: bool,
}

pub struct InMemoryStorage<const N: usize> {
    pub buf: [u8; N],
}

impl<const N: usize> Default for InMemoryStorage<N> {
    fn default() -> Self {
        Self { buf: [0u8; N] }
    }
}

impl<const N: usize> ReadStorage for InMemoryStorage<N> {
    type Error = ();

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let size = offset + bytes.len();
        bytes.copy_from_slice(&self.buf[offset..size]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> Storage for InMemoryStorage<N> {
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let size = offset + bytes.len();
        self.buf[offset..size].copy_from_slice(bytes);
        Ok(())
    }
}

impl<F> MockNlme<F>
where
    F: FnMut(NetworkSteeringEvent),
{
    fn new(callback: F) -> Self {
        Self {
            callback,
            network_discovery_called: false,
            network_formation_called: false,
        }
    }
}

impl<F> NlmeSap for MockNlme<F>
where
    F: FnMut(NetworkSteeringEvent),
{
    fn network_discovery(
        &self,
        _request: NlmeNetworkDiscoveryRequest,
    ) -> NlmeNetworkDiscoveryConfirm {
        // TODO: does it need to be mutable just for testing?
        // (self.callback)(NetworkSteeringEvent::ScanStarted);

        NlmeNetworkDiscoveryConfirm { 
            status: Successful, 
            network_count: 0u8, 
            network_descriptor: NetworkDescriptor {
                extended_pan_id: 0x123456789u64,
                pan_id: 0x1234u16,
                update_id: 1u8,
                logical_channel: 0u8,
                stack_profile: 2,
                zigbee_version: 0x22,
                beacon_order: 1u8,
                superframe_order: 1u8,
                permit_joining: true,
                router_capacity: true,
                end_device_capacity: false,
            }
        }
    }

    fn network_formation(
        &self,
        request: zigbee::nwk::nlme::management::NlmeNetworkFormationRequest,
    ) -> zigbee::nwk::nlme::management::NlmeNetworkFormationConfirm {
        todo!()
    }

    fn permit_joining(&self, request: zigbee::nwk::nlme::management::NlmePermitJoiningRequest) -> zigbee::nwk::nlme::management::NlmePermitJoiningConfirm {
        todo!()
    }

    fn start_router(&self, request: zigbee::nwk::nlme::management::NlmeStartRouterRequest) -> zigbee::nwk::nlme::management::NlmeStartRouterConfirm {
        todo!()
    }

    fn ed_scan(&self, request: zigbee::nwk::nlme::management::NlmeEdScanRequest) -> zigbee::nwk::nlme::management::NlmeEdScanConfirm {
        todo!()
    }

    fn join(&self, request: zigbee::nwk::nlme::management::NlmeJoinRequest) -> zigbee::nwk::nlme::management::NlmeJoinConfirm {
        todo!()
    }
}


pub const STORAGE_SIZE: usize = 15;

#[test]
fn test_initialization_procedure() {
    // given
    let storage = InMemoryStorage::<STORAGE_SIZE>::default();
    let mut events = Vec::new();
    let callback = |event| events.push(event);
    let nlme = MockNlme::new(callback);
    let config = zigbee::Config::default();
    let bdb_commisioning_capability = 0u8;
    let mut bdb = BaseDeviceBehavior::new(storage, nlme, config, bdb_commisioning_capability);

    // when
    let result = bdb.start_initialization_procedure();

    // then
    assert!(result.is_ok());
    // TODO: is callback triggered 
}
