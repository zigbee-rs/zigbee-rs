use zigbee_base_device_behavior::ZigbeeStack;
use zigbee_base_device_behavior::ZigbeeStackError;

#[derive(Default)]
pub struct MockStack {
    pub rejoin_called: bool,
    pub last_secure: Option<bool>,
}

impl ZigbeeStack for MockStack {
    fn start_network_scan(&mut self) -> Result<(), ZigbeeStackError> {
        Ok(())
    }
    fn join_network(
        &mut self,
        _descriptor: &zigbee_base_device_behavior::NetworkDescriptor,
    ) -> Result<(), ZigbeeStackError> {
        Ok(())
    }
    fn form_network(
        &mut self,
        _config: &zigbee_base_device_behavior::NetworkFormationConfig,
    ) -> Result<(), ZigbeeStackError> {
        Ok(())
    }
    fn leave_network(&mut self) -> Result<(), ZigbeeStackError> {
        Ok(())
    }
    fn rejoin_network(&mut self, secure: bool) -> Result<(), ZigbeeStackError> {
        self.rejoin_called = true;
        self.last_secure = Some(secure);
        Ok(())
    }
}

#[test]
fn test_rejoin_flow() {
    let mut stack = MockStack::default();
    assert!(!stack.rejoin_called);
    let result = stack.rejoin_network(true);
    assert!(result.is_ok());
    assert!(stack.rejoin_called);
    assert_eq!(stack.last_secure, Some(true));
}
