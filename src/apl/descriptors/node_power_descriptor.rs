//! 2.3.2.4 Node Power Descriptor
//!
//! The node power descriptor gives a dynamic indication of the power status of
//! the node and is mandatory for each node. There shall be only one node power
//! descriptor in a node.

use heapless::FnvIndexSet;
use heapless::Vec;
use strum::EnumCount;

use crate::apl::descriptors::error::Error;

const NODE_POWER_DESCRIPTOR_SIZE: usize = 2;

#[derive(Debug)]
pub struct NodePowerDescriptor(Vec<u8, NODE_POWER_DESCRIPTOR_SIZE>);

impl NodePowerDescriptor {
    fn new(
        current_power_mode: CurrentPowerMode,
        available_power_sources: AvailablePowerSources,
        current_power_source: CurrentPowerSource,
        current_power_source_level: CurrentPowerSourceLevel,
    ) -> Result<Self, Error> {
        let power_source = match current_power_source {
            CurrentPowerSource::ConstantMainPower => AvailablePowerSourcesFlag::ConstantMainPower,
            CurrentPowerSource::RechargeableBattery => {
                AvailablePowerSourcesFlag::RechargeableBattery
            }
            CurrentPowerSource::DisposableBattery => AvailablePowerSourcesFlag::DisposableBattery,
        };

        if !available_power_sources.is_set(power_source) {
            return Err(Error::CurrentPowerSourceNotAvailable);
        }

        let mut value: Vec<u8, NODE_POWER_DESCRIPTOR_SIZE> = Vec::new();
        value.resize(NODE_POWER_DESCRIPTOR_SIZE, 0).unwrap();

        value[0] |= (current_power_mode as u8) << 4;
        value[0] |= available_power_sources.0 as u8;

        value[1] |= (current_power_source as u8) << 4;
        value[1] |= current_power_source_level as u8;

        Ok(NodePowerDescriptor(value))
    }

    fn current_power_mode(&self) -> CurrentPowerMode {
        let current_power_mode = self.0[0] >> 4;
        current_power_mode.into()
    }

    fn available_power_sources(&self) -> AvailablePowerSources {
        AvailablePowerSources(self.0[0] & 0b1111)
    }

    fn current_power_source(&self) -> CurrentPowerSource {
        let current_power_source = self.0[1] >> 4;
        current_power_source.into()
    }

    fn current_power_source_level(&self) -> CurrentPowerSourceLevel {
        let current_power_source_level = self.0[1] & 0b1111;
        current_power_source_level.into()
    }
}

// 2.3.2.4.1 Current Power Mode Field
#[repr(u8)]
#[derive(Debug, PartialEq)]
pub enum CurrentPowerMode {
    // Receiver synchronized with the receiver on when  idle subfield of the node descriptor.
    Synchronized = 0b0000,
    // Receiver comes on periodically as defined by the  node power descriptor.
    Periodically = 0b0001,
    // Receiver comes on when stimulated, for example,  by a user pressing a button.
    Stimulated = 0b0010,
    // 0011 - 1111 reserved
}

impl From<u8> for CurrentPowerMode {
    fn from(value: u8) -> Self {
        match value {
            0b0000 => Self::Synchronized,
            0b0001 => Self::Periodically,
            0b0010 => Self::Stimulated,
            _ => panic!("{}", "Invalid CurrentPowerMode value: {value}"),
        }
    }
}

// 2.3.2.4.2 Available Power Sources Field
pub struct AvailablePowerSources(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq, EnumCount)]
pub enum AvailablePowerSourcesFlag {
    ConstantMainPower = 0,
    RechargeableBattery = 1,
    DisposableBattery = 2,
    // 3 reserved
}

impl AvailablePowerSources {
    fn new(
        available_power_sources_flags: FnvIndexSet<
            AvailablePowerSourcesFlag,
            { AvailablePowerSourcesFlag::COUNT.next_power_of_two() },
        >,
    ) -> Self {
        let mut value: u8 = 0;
        for available_power_source in available_power_sources_flags.iter() {
            value |= 1 << *available_power_source as u8;
        }

        Self(value)
    }

    fn is_set(&self, power_source: AvailablePowerSourcesFlag) -> bool {
        return (self.0 & (1 << power_source as u8)) != 0;
    }
}

// 2.3.2.4.3 Current Power Source Field
#[repr(u8)]
#[derive(Debug, PartialEq)]
pub enum CurrentPowerSource {
    ConstantMainPower = 0,
    RechargeableBattery = 1,
    DisposableBattery = 2,
    // 3 reserved
}

impl From<u8> for CurrentPowerSource {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::ConstantMainPower,
            1 => Self::RechargeableBattery,
            2 => Self::DisposableBattery,
            _ => panic!("{}", "Invalid CurrentPowerMode value: {value}"),
        }
    }
}

// 2.3.2.4.4 Current Power Source Level Field
#[repr(u8)]
#[derive(Debug, PartialEq)]
pub enum CurrentPowerSourceLevel {
    Critical = 0b0000,
    OneThird = 0b0100,
    TwoThirds = 0b1000,
    Full = 0b1100,
    // All other values reserved
}

impl From<u8> for CurrentPowerSourceLevel {
    fn from(value: u8) -> Self {
        match value {
            0b0000 => Self::Critical,
            0b0100 => Self::OneThird,
            0b1000 => Self::TwoThirds,
            0b1100 => Self::Full,
            _ => panic!("{}", "Invalid CurrentPowerSourceLevel value: {value}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::types::macros::bitfield_bits;

    #[test]
    fn creating_available_power_sources_should_succeed() {
        // given
        let expected: u8 = 0b0101;

        // when
        let bits = bitfield_bits!(
            AvailablePowerSourcesFlag;
            AvailablePowerSourcesFlag::ConstantMainPower,
            AvailablePowerSourcesFlag::DisposableBattery,
        );
        let available_power_sources = AvailablePowerSources::new(bits);

        // then
        assert_eq!(expected, available_power_sources.0);
    }

    #[test]
    fn reading_available_power_sources_should_succeed() {
        // given
        let bits = bitfield_bits!(
            AvailablePowerSourcesFlag;
            AvailablePowerSourcesFlag::ConstantMainPower,
            AvailablePowerSourcesFlag::DisposableBattery,
        );

        // when
        let available_power_sources = AvailablePowerSources::new(bits);

        // then
        assert!(available_power_sources.is_set(AvailablePowerSourcesFlag::ConstantMainPower));
        assert!(available_power_sources.is_set(AvailablePowerSourcesFlag::DisposableBattery));
        assert!(!available_power_sources.is_set(AvailablePowerSourcesFlag::RechargeableBattery));
    }

    #[test]
    fn creating_node_power_descriptor_should_succeed() {
        // given
        let current_power_mode = CurrentPowerMode::Synchronized;
        let available_power_sources_flags = bitfield_bits!(
            AvailablePowerSourcesFlag;
            AvailablePowerSourcesFlag::ConstantMainPower,
        );
        let available_power_sources = AvailablePowerSources::new(available_power_sources_flags);
        let current_power_source = CurrentPowerSource::ConstantMainPower;
        let current_power_source_level = CurrentPowerSourceLevel::TwoThirds;

        // when
        let node_power_descriptor = NodePowerDescriptor::new(
            current_power_mode,
            available_power_sources,
            current_power_source,
            current_power_source_level,
        );

        // then
        assert!(node_power_descriptor.is_ok());
        let node_power_descriptor = node_power_descriptor.unwrap();

        assert_eq!(
            node_power_descriptor.current_power_mode(),
            CurrentPowerMode::Synchronized
        );
        assert!(node_power_descriptor
            .available_power_sources()
            .is_set(AvailablePowerSourcesFlag::ConstantMainPower));
        assert!(!node_power_descriptor
            .available_power_sources()
            .is_set(AvailablePowerSourcesFlag::DisposableBattery));
        assert_eq!(
            node_power_descriptor.current_power_source(),
            CurrentPowerSource::ConstantMainPower
        );
        assert_eq!(
            node_power_descriptor.current_power_source_level(),
            CurrentPowerSourceLevel::TwoThirds
        );
    }

    #[test]
    fn creating_node_power_descriptor_should_fail() {
        // given
        let current_power_mode = CurrentPowerMode::Synchronized;
        let available_power_sources_flags = bitfield_bits!(
            AvailablePowerSourcesFlag;
            AvailablePowerSourcesFlag::ConstantMainPower,
        );
        let available_power_sources = AvailablePowerSources::new(available_power_sources_flags);
        let current_power_source = CurrentPowerSource::DisposableBattery;
        let current_power_source_level = CurrentPowerSourceLevel::TwoThirds;

        // when
        let node_power_descriptor = NodePowerDescriptor::new(
            current_power_mode,
            available_power_sources,
            current_power_source,
            current_power_source_level,
        );

        // then
        assert!(node_power_descriptor.is_err());
        assert_eq!(
            node_power_descriptor.unwrap_err(),
            Error::CurrentPowerSourceNotAvailable
        )
    }
}
