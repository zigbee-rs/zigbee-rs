//! 2.3.2.3 Node Descriptor
//!
//! The node descriptor contains information about the capabilities of the ZigBee node and is mandatory for each node.  There shall be only one node descriptor in a node.  

use heapless::FnvIndexSet;
use heapless::Vec;
use strum::EnumCount;

use crate::common::types::macros::bitfield_bits;

const NODE_DESCRIPTOR_SIZE: usize = 13;

pub struct NodeDescriptor(Vec<u8, NODE_DESCRIPTOR_SIZE>);

impl NodeDescriptor {
    fn new(
        logical_type: LogicalType,
        complex_descriptor_available: bool,
        user_descriptor_available: bool,
        //aps_flags: unsupported for now
        frequency_bands: FrequencyBands,
        mac_capabilities: MacCapabilities,
        manufacturer_code: u16,
        maximum_buffer_size: u8,
        maximum_incoming_transfer_size: u16,
        server_mask: ServerMask,
        maximum_outgoing_transfer_size: u16,
        descriptor_capabilities: DescriptorCapabilities,
    ) -> Self {
        let mut value: Vec<u8, NODE_DESCRIPTOR_SIZE> = Vec::new();
        value.resize(NODE_DESCRIPTOR_SIZE, 0).unwrap();

        value[0] |= (logical_type as u8) << 5;
        value[0] |= (complex_descriptor_available as u8) << 4;
        value[0] |= (user_descriptor_available as u8) << 3;

        value[1] = frequency_bands.0;

        value[2] = mac_capabilities.0;

        value[3] = (manufacturer_code >> 8) as u8;
        value[4] = manufacturer_code as u8;

        value[5] = maximum_buffer_size;

        value[6] = (maximum_incoming_transfer_size >> 8) as u8;
        value[7] = maximum_incoming_transfer_size as u8;

        value[8] = (server_mask.0 >> 8) as u8;
        value[9] = server_mask.0 as u8;

        value[10] = (maximum_outgoing_transfer_size >> 8) as u8;
        value[11] = maximum_outgoing_transfer_size as u8;

        value[12] = descriptor_capabilities.0;

        NodeDescriptor(value)
    }

    pub fn logical_type(&self) -> LogicalType {
        let logical_type: u8 = (self.0[0] >> 5) & 0b111;
        logical_type.into()
    }

    pub fn complex_descriptor_available(&self) -> bool {
        ((self.0[0] >> 4) & 0b1) != 0
    }

    pub fn user_descriptor_available(&self) -> bool {
        ((self.0[0] >> 3) & 0b1) != 0
    }

    pub fn frequency_bands(&self) -> FrequencyBands {
        FrequencyBands(self.0[1] & 0b0001_1111)
    }

    pub fn mac_capabilities(&self) -> MacCapabilities {
        MacCapabilities(self.0[2])
    }

    pub fn manufacturer_code(&self) -> u16 {
        let upper = self.0[3];
        let lower = self.0[4];
        ((upper as u16) << 8) | lower as u16
    }

    pub fn maximum_buffer_size(&self) -> u8 {
        self.0[5]
    }

    pub fn maximum_incoming_transfer_size(&self) -> u16 {
        let upper = self.0[6];
        let lower = self.0[7];
        ((upper as u16) << 8) | lower as u16
    }

    pub fn server_mask(&self) -> ServerMask {
        let upper = self.0[8];
        let lower = self.0[9];
        ServerMask(((upper as u16) << 8) | lower as u16)
    }

    pub fn maximum_outgoing_transfer_size(&self) -> u16 {
        let upper = self.0[10];
        let lower = self.0[11];
        ((upper as u16) << 8) | lower as u16
    }

    pub fn descriptor_capabilities(&self) -> DescriptorCapabilities {
        DescriptorCapabilities(self.0[12])
    }
}

// 2.3.2.3.1 Logical Type Field
// The logical type field of the node descriptor is three bits in length and specifies the device type of the ZigBee node.
#[repr(u8)]
#[derive(Debug, Default, PartialEq)]
pub enum LogicalType {
    Coordinator = 0b000,
    Router = 0b001,
    #[default]
    EndDevice = 0b010,
    // 011 - 111 reserved
}

impl From<u8> for LogicalType {
    fn from(value: u8) -> Self {
        match value {
            0b000 => LogicalType::Coordinator,
            0b001 => LogicalType::Router,
            0b010 => LogicalType::EndDevice,
            _ => panic!("Invalid LogicalType value"),
        }
    }
}

// 2.3.2.3.4 APS Flags Field
// The APS flags field of the node descriptor is three bits in length and specifies the application support sub-layer capabilities of the node.
// This field is currently not supported and shall be set to zero.

// 2.3.2.3.5 Frequency Band Field
// The frequency band field of the node descriptor is five bits in length and specifies the frequency bands that are supported by the underlying IEEE 802.15.4 radio(s) utilized by the node.
// For each frequency band supported by any  physically present underlying IEEE 802.15.4 radio, the corresponding bit of the frequency band field, shall be set to 1.
// All other bits shall be set to 0.
pub struct FrequencyBands(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq, EnumCount)]
pub enum FrequencyBandFlag {
    /// 868 - 868.6 MHz
    Low = 0,
    // reserved = 1
    /// 902 - 928 MHz
    Mid = 2,
    /// 2400 - 2483.5 MHz
    High = 3,
    /// European FSK sub-GHz bands: (863-876MHz and 915-921MHz)  
    EuropeanFSK = 4,
}

impl FrequencyBands {
    fn new(
        frequency_band_flags: FnvIndexSet<
            FrequencyBandFlag,
            { FrequencyBandFlag::COUNT.next_power_of_two() },
        >,
    ) -> Self {
        let mut value: u8 = 0;
        for frequency_band in frequency_band_flags.iter() {
            value |= 1 << *frequency_band as u8;
        }

        Self(value)
    }

    fn is_set(&self, frequency_band_flag: FrequencyBandFlag) -> bool {
        return (self.0 & (1 << frequency_band_flag as u8)) != 0;
    }
}

// 2.3.2.3.6 MAC Capability Flags Field
// The MAC capability flags field is eight bits in length and specifies the node capabilities, as required by the IEEE  802.15.4-2015 MAC sub-layer [B1].
pub struct MacCapabilities(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq, EnumCount)]
pub enum MacCapabilityFlag {
    /// The alternate PAN coordinator sub-field is one bit in length and shall be set to 1 if this node is capable of becoming a PAN coordinator.
    /// Otherwise, the alternative PAN coordinator sub-field shall be set to 0.
    AlternatePanCoordinator = 0,
    /// The device type sub-field is one bit in length and shall be set to 1 if this node is a full function device (FFD).
    /// Otherwise, the device type sub-field shall be set to 0, indicating a reduced function device (RFD).
    DeviceType = 1,
    /// The power source sub-field is one bit in length and shall be set to 1 if the current power source is mains power.
    /// Otherwise, the power source sub-field shall be set to 0.
    /// This information is derived from the node current power source field of the node power descriptor.
    PowerSource = 2,
    /// The receiver on when idle sub-field is one bit in length and shall be set to 1 if the device does not disable its receiver to conserve power during idle periods.
    /// Otherwise, the receiver on when idle sub-field shall be set to 0.
    ReceiverOnWhenIdle = 3,
    /// The security capability sub-field is one bit in length and shall be set to 1 if the device is capable of sending
    /// and receiving frames secured using the security suite specified in [B1].
    /// Otherwise, the security capability sub-field shall be set to 0.
    SecurityCapability = 6,
    /// The allocate address sub-field is one bit in length and shall be set to 0 or 1
    AllocateAddress = 7,
}

impl MacCapabilities {
    // Note: Capacity of IndexSet must be a power of 2.
    fn new(
        capability_flags: FnvIndexSet<
            MacCapabilityFlag,
            { MacCapabilityFlag::COUNT.next_power_of_two() },
        >,
    ) -> Self {
        let mut value: u8 = 0;
        for capa in capability_flags.iter() {
            value |= 1 << *capa as u8
        }

        Self(value)
    }

    fn is_set(&self, mac_capability_flag: MacCapabilityFlag) -> bool {
        return (self.0 & (1 << mac_capability_flag as u8)) != 0;
    }
}

pub struct ServerMask(u16);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq, EnumCount)]
pub enum ServerMaskFlag {
    PrimaryTrustCenter = 0,
    BackupTrustCenter = 1,
    PrimaryBindingTableCache = 2,
    BackupBindingTableCache = 3,
    PrimaryDiscoveryCache = 4,
    BackupDiscoveryCache = 5,
    NetworkManager = 6,
}

impl ServerMask {
    fn new(
        server_mask_flags: FnvIndexSet<
            ServerMaskFlag,
            { ServerMaskFlag::COUNT.next_power_of_two() },
        >,
        stack_compliance_revision: u8,
    ) -> Self {
        let mut value: u16 = 0;
        for bit in server_mask_flags.iter() {
            value |= 1 << *bit as u16
        }

        value |= (stack_compliance_revision as u16) << 9;

        Self(value)
    }

    fn is_set(&self, server_mask_flag: ServerMaskFlag) -> bool {
        return self.0 & (1 << server_mask_flag as u16) != 0;
    }

    fn get_stack_compliance_revision(&self) -> u8 {
        return (self.0 >> 9) as u8;
    }
}

pub struct DescriptorCapabilities(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq, EnumCount)]
pub enum DescriptorCapabilityFlag {
    ExtendedActiveEndpontListAvailable = 0,
    ExtendedSimpleDescriptorListAvailable = 1,
    // 2 -7 reserved
}

impl DescriptorCapabilities {
    fn new(
        descriptor_capability_flags: FnvIndexSet<
            DescriptorCapabilityFlag,
            { DescriptorCapabilityFlag::COUNT.next_power_of_two() },
        >,
    ) -> Self {
        let mut value: u8 = 0;
        for descriptor_capability in descriptor_capability_flags.iter() {
            value |= 1 << *descriptor_capability as u8
        }

        Self(value)
    }

    fn is_set(&self, descriptor_capability_flag: DescriptorCapabilityFlag) -> bool {
        return (self.0 & (1 << descriptor_capability_flag as u8)) != 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_frequency_bands_should_succeed() {
        // given
        let expected: u8 = 0b0001_0100;

        // when
        let bits = bitfield_bits!(
            FrequencyBandFlag;
            FrequencyBandFlag::Mid,
            FrequencyBandFlag::EuropeanFSK,
        );
        let freqyency_bands = FrequencyBands::new(bits);

        // then
        assert_eq!(expected, freqyency_bands.0);
    }

    #[test]
    fn reading_frequency_bands_should_succeed() {
        // given
        let bits = bitfield_bits!(
            FrequencyBandFlag;
            FrequencyBandFlag::Mid,
            FrequencyBandFlag::EuropeanFSK,
        );

        // when
        let freqyency_bands = FrequencyBands::new(bits);

        // then
        assert!(freqyency_bands.is_set(FrequencyBandFlag::Mid));
        assert!(freqyency_bands.is_set(FrequencyBandFlag::EuropeanFSK));
        assert!(!freqyency_bands.is_set(FrequencyBandFlag::Low));
    }

    #[test]
    fn creating_mac_capabilities_should_succeed() {
        // given
        let expected: u8 = 0b1000_0001;

        // when
        let bits = bitfield_bits!(
            MacCapabilityFlag;
            MacCapabilityFlag::AlternatePanCoordinator,
            MacCapabilityFlag::AllocateAddress,
        );
        let mac_capabilities = MacCapabilities::new(bits);

        // then
        assert_eq!(expected, mac_capabilities.0);
    }

    #[test]
    fn reading_mac_capabilities_should_succeed() {
        // given
        let bits = bitfield_bits!(
            MacCapabilityFlag;
            MacCapabilityFlag::AlternatePanCoordinator,
            MacCapabilityFlag::AllocateAddress,
        );

        // when
        let mac_capabilities = MacCapabilities::new(bits);

        // then
        assert!(mac_capabilities.is_set(MacCapabilityFlag::AlternatePanCoordinator));
        assert!(mac_capabilities.is_set(MacCapabilityFlag::AllocateAddress));
        assert!(!mac_capabilities.is_set(MacCapabilityFlag::DeviceType));
    }

    #[test]
    fn creating_server_mask_should_succeed() {
        // given
        let expected = 0b0010_1100_0100_0001;

        // when
        let bits = bitfield_bits!(
            ServerMaskFlag;
            ServerMaskFlag::PrimaryTrustCenter,
            ServerMaskFlag::NetworkManager,
        );
        let server_mask = ServerMask::new(bits, 22);

        // then
        assert_eq!(expected, server_mask.0);
    }

    #[test]
    fn reading_server_mask_should_succeed() {
        // given
        let bits = bitfield_bits!(
            ServerMaskFlag;
            ServerMaskFlag::PrimaryTrustCenter,
            ServerMaskFlag::NetworkManager,
        );

        // when
        let server_mask = ServerMask::new(bits, 22);

        // then
        assert!(server_mask.is_set(ServerMaskFlag::PrimaryTrustCenter));
        assert!(server_mask.is_set(ServerMaskFlag::NetworkManager));
        assert!(!server_mask.is_set(ServerMaskFlag::PrimaryDiscoveryCache));
        assert_eq!(22, server_mask.get_stack_compliance_revision());
    }

    #[test]
    fn creating_descriptor_capability_should_succeed() {
        // given
        let expected = 0b0000_0011;

        // when
        let bits = bitfield_bits!(
            DescriptorCapabilityFlag;
            DescriptorCapabilityFlag::ExtendedActiveEndpontListAvailable,
            DescriptorCapabilityFlag::ExtendedSimpleDescriptorListAvailable,
        );
        let descriptor_capabilities = DescriptorCapabilities::new(bits);

        // then
        assert_eq!(expected, descriptor_capabilities.0);
    }

    #[test]
    fn reading_descriptor_capability_should_succeed() {
        // given
        let bits = bitfield_bits!(
            DescriptorCapabilityFlag;
            DescriptorCapabilityFlag::ExtendedActiveEndpontListAvailable,
            DescriptorCapabilityFlag::ExtendedSimpleDescriptorListAvailable,
        );

        // when
        let descriptor_capabilities = DescriptorCapabilities::new(bits);

        // then
        assert!(descriptor_capabilities
            .is_set(DescriptorCapabilityFlag::ExtendedActiveEndpontListAvailable));
        assert!(descriptor_capabilities
            .is_set(DescriptorCapabilityFlag::ExtendedSimpleDescriptorListAvailable));
    }

    #[test]
    fn creating_node_descriptor_should_succeed() {
        // given
        let logical_type = LogicalType::Router;
        let complex_descriptor_available = true;
        let user_descriptor_available = true;
        let frequency_band_flags = bitfield_bits!(
            FrequencyBandFlag;
            FrequencyBandFlag::High,
        );
        let frequency_bands = FrequencyBands::new(frequency_band_flags);
        let mac_capability_flags = bitfield_bits!(
            MacCapabilityFlag;
            MacCapabilityFlag::AllocateAddress,
            MacCapabilityFlag::SecurityCapability,
        );
        let mac_capabilities = MacCapabilities::new(mac_capability_flags);
        let manufacturer_code = 42;
        let maximum_buffer_size = 8;
        let maximum_incoming_transfer_size = 500;
        let server_mask_flags = bitfield_bits!(
            ServerMaskFlag;
            ServerMaskFlag::PrimaryTrustCenter,
            ServerMaskFlag::BackupBindingTableCache,
        );
        let stack_compliance_revision = 14;
        let server_mask = ServerMask::new(server_mask_flags, stack_compliance_revision);
        let maximum_outgoing_transfer_size = 1000;
        let descriptor_capability_flags = bitfield_bits!(
            DescriptorCapabilityFlag;
            DescriptorCapabilityFlag::ExtendedActiveEndpontListAvailable,
        );
        let descriptor_capabilities = DescriptorCapabilities::new(descriptor_capability_flags);

        // when
        let node_descriptor = NodeDescriptor::new(
            logical_type,
            complex_descriptor_available,
            user_descriptor_available,
            frequency_bands,
            mac_capabilities,
            manufacturer_code,
            maximum_buffer_size,
            maximum_incoming_transfer_size,
            server_mask,
            maximum_outgoing_transfer_size,
            descriptor_capabilities,
        );

        // then
        assert_eq!(node_descriptor.logical_type(), LogicalType::Router);
        assert!(node_descriptor.complex_descriptor_available());
        assert!(node_descriptor.user_descriptor_available());
        assert!(node_descriptor
            .frequency_bands()
            .is_set(FrequencyBandFlag::High));
        assert!(!node_descriptor
            .frequency_bands()
            .is_set(FrequencyBandFlag::EuropeanFSK));
        assert!(node_descriptor
            .mac_capabilities()
            .is_set(MacCapabilityFlag::AllocateAddress));
        assert!(node_descriptor
            .mac_capabilities()
            .is_set(MacCapabilityFlag::SecurityCapability));
        assert!(!node_descriptor
            .mac_capabilities()
            .is_set(MacCapabilityFlag::PowerSource));
        assert_eq!(node_descriptor.manufacturer_code(), 42);
        assert_eq!(node_descriptor.maximum_buffer_size(), 8);
        assert_eq!(node_descriptor.maximum_incoming_transfer_size(), 500);
        assert!(node_descriptor
            .server_mask()
            .is_set(ServerMaskFlag::PrimaryTrustCenter));
        assert!(node_descriptor
            .server_mask()
            .is_set(ServerMaskFlag::BackupBindingTableCache));
        assert!(!node_descriptor
            .server_mask()
            .is_set(ServerMaskFlag::NetworkManager));
        assert_eq!(node_descriptor.maximum_outgoing_transfer_size(), 1000);
        assert!(node_descriptor
            .descriptor_capabilities()
            .is_set(DescriptorCapabilityFlag::ExtendedActiveEndpontListAvailable));
        assert!(!node_descriptor
            .descriptor_capabilities()
            .is_set(DescriptorCapabilityFlag::ExtendedSimpleDescriptorListAvailable));
    }
}
