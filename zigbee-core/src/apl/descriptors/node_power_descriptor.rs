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

// every node power descriptor field is 4 bits wide
const NIBBLE: u8 = 0b1111;

#[derive(Debug)]
pub struct NodePowerDescriptor<'a> {
    bytes: &'a [u8],
}

impl<'a> TryRead<'a, byte::ctx::Endian> for NodePowerDescriptor<'a> {
    fn try_read(bytes: &'a [u8], endian: byte::ctx::Endian) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let byte: u8 = bytes.read_with(offset, endian)?;
        let available_power_sources = byte >> 4;

        let byte: u8 = bytes.read_with(offset, endian)?;
        let current_power_source = PowerSource::try_read(&[byte & NIBBLE], ()).unwrap().0;

        if let PowerSource::Reserved(_) = current_power_source {
            return Err(byte::Error::BadInput {
                err: "CurrentPowerSourceNotAvailable: No curent power source set",
            });
        }

        if available_power_sources & current_power_source.bits() != 0 {
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
        CurrentPowerMode::try_read(&[self.bytes[0] & NIBBLE], ())
            .unwrap()
            .0
    }

    /// Whether the node declares `power_source` as available (2.3.2.4.2).
    pub fn supports(&self, power_source: PowerSource) -> bool {
        (self.bytes[0] >> 4) & power_source.bits() != 0
    }

    fn current_power_source(&self) -> PowerSource {
        PowerSource::try_read(&[self.bytes[1] & NIBBLE], ())
            .unwrap()
            .0
    }

    fn current_power_source_level(&self) -> CurrentPowerSourceLevel {
        CurrentPowerSourceLevel::try_read(&[self.bytes[1] >> 4], ())
            .unwrap()
            .0
    }
}

impl_byte! {
    #[tag(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// Current power mode field (2.3.2.4.1).
    pub enum CurrentPowerMode {
        /// Synchronized with the receiver-on-when-idle subfield of the node descriptor.
        Synchronized = 0b0000,
        /// Comes on periodically as defined by the node power descriptor.
        Periodically = 0b0001,
        /// Comes on when stimulated, e.g. by a user pressing a button.
        Stimulated = 0b0010,
        #[fallback = true]
        Reserved(u8),
    }
}

impl CurrentPowerMode {
    /// The 4-bit field value.
    pub const fn bits(self) -> u8 {
        match self {
            Self::Synchronized => 0b0000,
            Self::Periodically => 0b0001,
            Self::Stimulated => 0b0010,
            Self::Reserved(bits) => bits & NIBBLE,
        }
    }
}

impl_byte! {
    #[tag(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// A power source of the node, as its bit in the available power sources
    /// (2.3.2.4.2) and current power source (2.3.2.4.3) fields, which share a
    /// bit assignment.
    pub enum PowerSource {
        ConstantMainPower = 0b0001,
        RechargeableBattery = 0b0010,
        DisposableBattery = 0b0100,
        #[fallback = true]
        Reserved(u8),
    }
}

impl PowerSource {
    /// The 4-bit field value with this source's bit set.
    pub const fn bits(self) -> u8 {
        match self {
            Self::ConstantMainPower => 0b0001,
            Self::RechargeableBattery => 0b0010,
            Self::DisposableBattery => 0b0100,
            Self::Reserved(bits) => bits & NIBBLE,
        }
    }

    /// The 4-bit field value covering every source in `sources`.
    pub const fn bitmap(sources: &[Self]) -> u8 {
        let mut bits = 0;
        let mut i = 0;
        while i < sources.len() {
            bits |= sources[i].bits();
            i += 1;
        }
        bits
    }
}

impl_byte! {
    #[tag(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// Current power source level field (2.3.2.4.4).
    pub enum CurrentPowerSourceLevel {
        Critical = 0b0000,
        OneThird = 0b0100,
        TwoThirds = 0b1000,
        Full = 0b1100,
        #[fallback = true]
        Reserved(u8),
    }
}

impl CurrentPowerSourceLevel {
    /// The 4-bit field value.
    pub const fn bits(self) -> u8 {
        match self {
            Self::Critical => 0b0000,
            Self::OneThird => 0b0100,
            Self::TwoThirds => 0b1000,
            Self::Full => 0b1100,
            Self::Reserved(bits) => bits & NIBBLE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_node_power_descriptor_should_succeed() {
        let bytes: [u8; 2] = [0x50, 0x84];

        let node_power_descriptor = NodePowerDescriptor::try_read(&bytes, byte::LE);

        assert!(node_power_descriptor.is_ok());
        let node_power_descriptor = node_power_descriptor.unwrap().0;
        assert_eq!(
            node_power_descriptor.current_power_mode(),
            CurrentPowerMode::Synchronized
        );
        assert!(node_power_descriptor.supports(PowerSource::DisposableBattery));
        assert!(node_power_descriptor.supports(PowerSource::ConstantMainPower));
        assert!(!node_power_descriptor.supports(PowerSource::RechargeableBattery));
        assert_eq!(
            node_power_descriptor.current_power_source(),
            PowerSource::DisposableBattery
        );
        assert_eq!(
            node_power_descriptor.current_power_source_level(),
            CurrentPowerSourceLevel::TwoThirds
        );
    }

    #[test]
    fn creating_node_power_descriptor_should_fail() {
        let bytes: [u8; 2] = [0x50, 0x82];

        let node_power_descriptor = NodePowerDescriptor::try_read(&bytes, byte::LE);
        assert!(node_power_descriptor.is_err());
        assert_eq!(
            node_power_descriptor.unwrap_err(),
            byte::Error::BadInput {
                err: "CurrentPowerSourceNotAvailable: Current power source not in available power sources",
            },
        );
    }
}
