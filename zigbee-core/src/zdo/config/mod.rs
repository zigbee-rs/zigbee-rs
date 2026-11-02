/// zigbee configuration
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Config {
    /// The radio channel to operate on
    pub radio_channel: u8,
    /// Discovery type
    pub device_discovery_type: DiscoveryType,
    /// The device class
    pub device_type: crate::apl::descriptors::node_descriptor::LogicalType,
}

/// Discovery Type
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DiscoveryType {
    #[default]
    /// The IEEE address request is unicast to a particular device and assumes
    /// the NWK address is known.
    IEEE,
    /// The NWK address request is broadcast and carries the known IEEE address
    /// as data payload.
    NWK,
}
