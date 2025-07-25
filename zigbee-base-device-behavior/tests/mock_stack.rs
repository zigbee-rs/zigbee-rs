use zigbee_base_device_behavior::NetworkDescriptor;
use zigbee_base_device_behavior::NetworkFormationConfig;
use zigbee_base_device_behavior::ZigbeeStack;
use zigbee_base_device_behavior::ZigbeeStackError;

#[derive(Default)]
pub struct MockStack {
    pub scan_called: bool,
}

impl ZigbeeStack for MockStack {
    fn start_network_scan(&mut self) -> Result<(), ZigbeeStackError> {
        self.scan_called = true;
        Ok(())
    }
    fn join_network(&mut self, _descriptor: &NetworkDescriptor) -> Result<(), ZigbeeStackError> {
        Err(ZigbeeStackError::NotSupported)
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

#[test]
fn test_mock_stack_scan() {
    let mut stack = MockStack::default();
    assert!(!stack.scan_called);
    let result = stack.start_network_scan();
    assert!(result.is_ok());
    assert!(stack.scan_called);
}
