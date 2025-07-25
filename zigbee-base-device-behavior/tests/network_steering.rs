use zigbee_base_device_behavior::types::NetworkSteering;
use zigbee_base_device_behavior::types::NetworkSteeringEvent;
use zigbee_base_device_behavior::NetworkDescriptor;
use zigbee_base_device_behavior::NetworkFormationConfig;
use zigbee_base_device_behavior::ZigbeeStack;
use zigbee_base_device_behavior::ZigbeeStackError;

#[derive(Default)]
pub struct MockStack {
    pub scan_called: bool,
    pub join_called: bool,
}

impl ZigbeeStack for MockStack {
    fn start_network_scan(&mut self) -> Result<(), ZigbeeStackError> {
        self.scan_called = true;
        Ok(())
    }
    fn join_network(&mut self, _descriptor: &NetworkDescriptor) -> Result<(), ZigbeeStackError> {
        self.join_called = true;
        Ok(())
    }
    fn form_network(&mut self, _config: &NetworkFormationConfig) -> Result<(), ZigbeeStackError> {
        Err(ZigbeeStackError::NotSupported)
    }
    fn leave_network(&mut self) -> Result<(), ZigbeeStackError> {
        Err(ZigbeeStackError::NotSupported)
    }
    fn rejoin_network(&mut self, _secure: bool) -> Result<(), ZigbeeStackError> {
        Err(ZigbeeStackError::NotSupported)
    }
}

struct DummySteering;

impl NetworkSteering for DummySteering {
    fn start_network_steering<F>(&mut self, mut event_callback: F)
    where
        F: FnMut(NetworkSteeringEvent),
    {
        // Simulate scan and join
        let desc = NetworkDescriptor {
            pan_id: 0x1234,
            extended_pan_id: [0; 8],
            channel: 11,
            lqi: 255,
            stack_profile: 2,
            zigbee_version: 0x22,
            permit_joining: true,
            depth: 1,
            router_capacity: true,
            end_device_capacity: true,
        };
        event_callback(NetworkSteeringEvent::ScanStarted);
        event_callback(NetworkSteeringEvent::ScanResult(desc.clone()));
        event_callback(NetworkSteeringEvent::JoinInProgress(desc.clone()));
        event_callback(NetworkSteeringEvent::JoinSuccess(desc));
    }
}

#[test]
fn test_network_steering_flow() {
    let mut events = Vec::new();
    let mut steering = DummySteering;
    steering.start_network_steering(|event| events.push(event));
    assert!(matches!(events[0], NetworkSteeringEvent::ScanStarted));
    assert!(matches!(events[1], NetworkSteeringEvent::ScanResult(_)));
    assert!(matches!(events[2], NetworkSteeringEvent::JoinInProgress(_)));
    assert!(matches!(events[3], NetworkSteeringEvent::JoinSuccess(_)));
}
