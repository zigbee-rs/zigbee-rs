//! Node Power Descriptor
//!
//! See Section 2.3.2.4
//!
//! The node power descriptor gives a dynamic indication of the power status of
//! the node and is mandatory for each node. There shall be only one node power
//! descriptor in a node.

use byte::BytesExt;
use byte::TryRead;
use zigbee_macros::impl_byte;

const NODE_POWER_DESCRIPTOR_SIZE: usize = 2;

#[derive(Debug)]
pub struct NodePowerDescriptor<'a> {
    bytes: &'a [u8],
}

impl<'a> TryRead<'a, byte::ctx::Endian> for NodePowerDescriptor<'a> {
    fn try_read(bytes: &'a [u8], endian: byte::ctx::Endian) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let byte: u8 = bytes.read_with(offset, endian)?;
        let available_power_sources = AvailablePowerSources(byte >> 4);

        let byte: u8 = bytes.read_with(offset, endian)?;
        let current_power_source = CurrentPowerSource::try_read(&[byte & 0b1111], ())
            .unwrap()
            .0;

        let power_source = match current_power_source {
            CurrentPowerSource::ConstantMainPower => AvailablePowerSourcesFlag::ConstantMainPower,
            CurrentPowerSource::RechargeableBattery => {
                AvailablePowerSourcesFlag::RechargeableBattery
            }
            CurrentPowerSource::DisposableBattery => AvailablePowerSourcesFlag::DisposableBattery,
            CurrentPowerSource::Reserved(_) => {
                return Err(byte::Error::BadInput {
                    err: "CurrentPowerSourceNotAvailable: No curent power source set",
                })
            }
        };

        if available_power_sources.is_set(power_source) {
            Ok((NodePowerDescriptor { bytes }, *offset))
        } else {
            Err(byte::Error::BadInput {
                err: "CurrentPowerSourceNotAvailable: Current power source not in available power sources",
            })
        }
    }
}

impl NodePowerDescriptor<'_> {
    fn current_power_mode(&self) -> CurrentPowerMode {
        CurrentPowerMode::try_read(&[self.bytes[0] & 0b1111], ())
            .unwrap()
            .0
    }

    fn available_power_sources(&self) -> AvailablePowerSources {
        AvailablePowerSources(self.bytes[0] >> 4)
    }

    fn current_power_source(&self) -> CurrentPowerSource {
        CurrentPowerSource::try_read(&[self.bytes[1] & 0b1111], ())
            .unwrap()
            .0
    }

    fn current_power_source_level(&self) -> CurrentPowerSourceLevel {
        CurrentPowerSourceLevel::try_read(&[self.bytes[1] >> 4], ())
            .unwrap()
            .0
    }
}

// 2.3.2.4.1 Current Power Mode Field
impl_byte! {
    #[tag(u8)]
    #[derive(Debug, PartialEq, Eq)]
    pub enum CurrentPowerMode {
        // Receiver synchronized with the receiver on when  idle subfield of the node descriptor.
        Synchronized = 0b0000,
        // Receiver comes on periodically as defined by the  node power descriptor.
        Periodically = 0b0001,
        // Receiver comes on when stimulated, for example,  by a user pressing a button.
        Stimulated = 0b0010,
        // 0011 - 1111 reserved
        #[fallback = true]
        Reserved(u8),
    }
}

// 2.3.2.4.2 Available Power Sources Field
pub struct AvailablePowerSources(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum AvailablePowerSourcesFlag {
    ConstantMainPower = 0,
    RechargeableBattery = 1,
    DisposableBattery = 2,
}

impl AvailablePowerSources {
    fn is_set(&self, power_source: AvailablePowerSourcesFlag) -> bool {
        (self.0 & (1 << power_source as u8)) != 0
    }
}

// 2.3.2.4.3 Current Power Source Field
impl_byte! {
    #[tag(u8)]
    #[derive(Debug, PartialEq, Eq)]
    pub enum CurrentPowerSource {
        ConstantMainPower = 0b000,
        RechargeableBattery = 0b010,
        DisposableBattery = 0b100,
        #[fallback = true]
        Reserved(u8),
    }
}

// 2.3.2.4.4 Current Power Source Level Field
impl_byte! {
    #[tag(u8)]
    #[derive(Debug, PartialEq, Eq)]
    pub enum CurrentPowerSourceLevel {
        Critical = 0b0000,
        OneThird = 0b0100,
        TwoThirds = 0b1000,
        Full = 0b1100,
        #[fallback = true]
        Reserved(u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_node_power_descriptor_should_succeed() {
        // given
        // current_power_mode = CurrentPowerMode::Synchronized
        // available_power_sources = { ConstantMainPower, DisposableBattery }
        // 01010000 = 0x50

        // current_power_source = CurrentPowerSource::DisposableBattery
        // current_power_source_level = CurrentPowerSourceLevel::TwoThirds
        // 10000100 = 0x84
        let bytes: [u8; 2] = [0x50, 0x84];

        // when
        let node_power_descriptor = NodePowerDescriptor::try_read(&bytes, byte::LE);

        // then
        assert!(node_power_descriptor.is_ok());
        let node_power_descriptor = node_power_descriptor.unwrap().0;
        assert_eq!(
            node_power_descriptor.current_power_mode(),
            CurrentPowerMode::Synchronized
        );
        assert!(node_power_descriptor
            .available_power_sources()
            .is_set(AvailablePowerSourcesFlag::DisposableBattery));
        assert!(node_power_descriptor
            .available_power_sources()
            .is_set(AvailablePowerSourcesFlag::ConstantMainPower));
        assert!(!node_power_descriptor
            .available_power_sources()
            .is_set(AvailablePowerSourcesFlag::RechargeableBattery));
        assert_eq!(
            node_power_descriptor.current_power_source(),
            CurrentPowerSource::DisposableBattery
        );
        assert_eq!(
            node_power_descriptor.current_power_source_level(),
            CurrentPowerSourceLevel::TwoThirds
        );
    }

    #[test]
    fn creating_node_power_descriptor_should_fail() {
        // given
        // current_power_mode = CurrentPowerMode::Synchronized
        // available_power_sources = { ConstantMainPower, DisposableBattery }
        // 01010000 = 0x50

        // current_power_source = CurrentPowerSource::RechargeableBattery
        // current_power_source_level = CurrentPowerSourceLevel::TwoThirds
        // 10000010 = 0x82
        let bytes: [u8; 2] = [0x50, 0x82];

        // when
        let node_power_descriptor = NodePowerDescriptor::try_read(&bytes, byte::LE);
        // then
        assert!(node_power_descriptor.is_err());
        assert_eq!(
            node_power_descriptor.unwrap_err(),
            byte::Error::BadInput{
                err: "CurrentPowerSourceNotAvailable: Current power source not in available power sources",
            },
        );
    }
}
