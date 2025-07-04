use crate::internal::macros::impl_byte;

pub struct RejoinRequest {
    pub capability_information: CapabilityInformation,
}

impl_byte! {
    /// Capability Information field for Rejoin Request
    /// See Zigbee spec, Section 3.4.3.5.1.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct CapabilityInformation(pub u8);
}

impl core::fmt::Debug for CapabilityInformation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CapabilityInformation")
            .field("device_type", &self.device_type())
            .field("power_source", &self.power_source())
            .field("receiver_on_when_idle", &self.receiver_on_when_idle())
            .field("allocate_address", &self.allocate_address())
            .finish()
    }
}

impl CapabilityInformation {
    /// Device type: 1 = router, 0 = end device
    pub fn device_type(&self) -> u8 {
        (self.0 & mask::DEVICE_TYPE) >> offset::DEVICE_TYPE
    }

    #[must_use]
    pub fn set_device_type(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::DEVICE_TYPE) | (value << offset::DEVICE_TYPE);
        self
    }

    /// Power source: 1 = mains, 0 = other
    pub fn power_source(&self) -> u8 {
        (self.0 & mask::POWER_SOURCE) >> offset::POWER_SOURCE
    }

    #[must_use]
    pub fn set_power_source(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::POWER_SOURCE) | (value << offset::POWER_SOURCE);
        self
    }

    /// Receiver on when idle: 1 = enabled, 0 = may be disabled
    pub fn receiver_on_when_idle(&self) -> u8 {
        (self.0 & mask::RECEIVER_ON_WHEN_IDLE) >> offset::RECEIVER_ON_WHEN_IDLE
    }

    #[must_use]
    pub fn set_receiver_on_when_idle(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::RECEIVER_ON_WHEN_IDLE) | (value << offset::RECEIVER_ON_WHEN_IDLE);
        self
    }

    /// Allocate address: 1 = must be issued a 16-bit address, 0 = self-selected
    pub fn allocate_address(&self) -> u8 {
        (self.0 & mask::ALLOCATE_ADDRESS) >> offset::ALLOCATE_ADDRESS
    }

    #[must_use]
    pub fn set_allocate_address(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::ALLOCATE_ADDRESS) | (value << offset::ALLOCATE_ADDRESS);
        self
    }
}

mod offset {
    pub const DEVICE_TYPE: u8 = 1;
    pub const POWER_SOURCE: u8 = 2;
    pub const RECEIVER_ON_WHEN_IDLE: u8 = 3;
    pub const ALLOCATE_ADDRESS: u8 = 7;
}

mod mask {
    pub const DEVICE_TYPE: u8 = 0b0000_0010;
    pub const POWER_SOURCE: u8 = 0b0000_0100;
    pub const RECEIVER_ON_WHEN_IDLE: u8 = 0b0000_1000;
    pub const ALLOCATE_ADDRESS: u8 = 0b1000_0000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_information() {
        let mut cap = CapabilityInformation(0);
        assert_eq!(cap.device_type(), 0);
        assert_eq!(cap.power_source(), 0);
        assert_eq!(cap.receiver_on_when_idle(), 0);
        assert_eq!(cap.allocate_address(), 0);

        cap = cap
            .set_device_type(1)
            .set_power_source(1)
            .set_receiver_on_when_idle(1)
            .set_allocate_address(1);
        assert_eq!(cap.device_type(), 1);
        assert_eq!(cap.power_source(), 1);
        assert_eq!(cap.receiver_on_when_idle(), 1);
        assert_eq!(cap.allocate_address(), 1);
    }
}
