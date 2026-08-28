//! Node Descriptor
//!
//! See Section 2.3.2.3
//!
//! The node descriptor contains information about the capabilities of the
//! ZigBee node and is mandatory for each node.  There shall be only one node
//! descriptor in a node.

use byte::TryRead;
use zigbee_macros::impl_byte;

const NODE_DESCRIPTOR_SIZE: usize = 13;

impl_byte! {
    pub struct NodeDescriptor<'a> {
        #[ctx = byte::ctx::Bytes::Len(NODE_DESCRIPTOR_SIZE)]
        #[ctx_write = ()]
        bytes: &'a [u8]
    }
}

impl NodeDescriptor<'_> {
    pub fn logical_type(&self) -> LogicalType {
        let logical_type: u8 = self.bytes[0] & 0b111;
        LogicalType::try_read(&[logical_type], ()).unwrap().0
    }

    pub fn complex_descriptor_available(&self) -> bool {
        ((self.bytes[0] >> 3) & 0b1) != 0
    }

    pub fn user_descriptor_available(&self) -> bool {
        ((self.bytes[0] >> 4) & 0b1) != 0
    }

    pub fn frequency_bands(&self) -> FrequencyBands {
        FrequencyBands(self.bytes[1] >> 3)
    }

    pub fn mac_capabilities(&self) -> MacCapabilities {
        MacCapabilities(self.bytes[2])
    }

    pub fn manufacturer_code(&self) -> u16 {
        let lower = self.bytes[3];
        let upper = self.bytes[4];
        (u16::from(upper) << 8) | u16::from(lower)
    }

    pub fn maximum_buffer_size(&self) -> u8 {
        self.bytes[5]
    }

    pub fn maximum_incoming_transfer_size(&self) -> u16 {
        let lower = self.bytes[6];
        let upper = self.bytes[7];
        (u16::from(upper) << 8) | u16::from(lower)
    }

    pub fn server_mask(&self) -> ServerMask {
        let lower = self.bytes[8];
        let upper = self.bytes[9];
        ServerMask((u16::from(upper) << 8) | u16::from(lower))
    }

    pub fn maximum_outgoing_transfer_size(&self) -> u16 {
        let lower = self.bytes[10];
        let upper = self.bytes[11];
        (u16::from(upper) << 8) | u16::from(lower)
    }

    pub fn descriptor_capabilities(&self) -> DescriptorCapabilities {
        DescriptorCapabilities(self.bytes[12])
    }
}

impl_byte! {
    #[tag(u8)]
    /// Logical type field (2.3.2.3.1): device type of the ZigBee node.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LogicalType {
        Coordinator = 0b000,
        Router = 0b001,
        EndDevice = 0b010,
        #[fallback = true]
        Reserved(u8),
    }
}

impl Default for LogicalType {
    fn default() -> Self {
        Self::Router
    }
}

// APS flags field (2.3.2.3.4): unsupported, always zero

/// Frequency band field (2.3.2.3.5): frequency bands supported by the
/// underlying IEEE 802.15.4 radio(s).
pub struct FrequencyBands(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum FrequencyBandFlag {
    /// 868 - 868.6 MHz
    Low = 0,
    /// 902 - 928 MHz
    Mid = 2,
    /// 2400 - 2483.5 MHz
    High = 3,
    /// European FSK sub-GHz bands: (863-876MHz and 915-921MHz)
    EuropeanFSK = 4,
}

impl FrequencyBands {
    fn is_set(&self, frequency_band_flag: FrequencyBandFlag) -> bool {
        (self.0 & (1 << frequency_band_flag as u8)) != 0
    }
}

/// MAC capability flags field (2.3.2.3.6), as required by the IEEE
/// 802.15.4-2015 MAC sub-layer.
pub struct MacCapabilities(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum MacCapabilityFlag {
    /// Set if this node is capable of becoming a PAN coordinator.
    AlternatePanCoordinator = 0,
    /// Set if this node is a full function device (FFD), unset for a reduced
    /// function device (RFD).
    DeviceType = 1,
    /// Set if the current power source is mains power.
    PowerSource = 2,
    /// Set if the device does not disable its receiver to conserve power
    /// during idle periods.
    ReceiverOnWhenIdle = 3,
    /// Set if the device supports the ZigBee security suite.
    SecurityCapability = 6,
    AllocateAddress = 7,
}

impl MacCapabilities {
    fn is_set(&self, mac_capability_flag: MacCapabilityFlag) -> bool {
        (self.0 & (1 << mac_capability_flag as u8)) != 0
    }
}

pub struct ServerMask(u16);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
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
    fn is_set(&self, server_mask_flag: ServerMaskFlag) -> bool {
        self.0 & (1 << server_mask_flag as u16) != 0
    }

    fn get_stack_compliance_revision(&self) -> u8 {
        (self.0 >> 9) as u8
    }
}

pub struct DescriptorCapabilities(u8);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum DescriptorCapabilityFlag {
    ExtendedActiveEndpontListAvailable = 0,
    ExtendedSimpleDescriptorListAvailable = 1,
}

impl DescriptorCapabilities {
    fn is_set(&self, descriptor_capability_flag: DescriptorCapabilityFlag) -> bool {
        (self.0 & (1 << descriptor_capability_flag as u8)) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apl::descriptors::node_descriptor;

    #[test]
    fn creating_node_descriptor_should_succeed() {
        let bytes = [
            0x19, 0x40, 0xC0, 0x2A, 0x00, 0x08, 0xF4, 0x01, 0x09, 0x2C, 0xE8, 0x03, 0x01,
        ];

        let node_descriptor = NodeDescriptor::try_read(&bytes, ()).unwrap().0;

        assert_eq!(node_descriptor.logical_type(), LogicalType::Router);
        assert!(node_descriptor.complex_descriptor_available());
        assert!(node_descriptor.user_descriptor_available());
        assert!(
            node_descriptor
                .frequency_bands()
                .is_set(FrequencyBandFlag::High)
        );
        assert!(
            !node_descriptor
                .frequency_bands()
                .is_set(FrequencyBandFlag::EuropeanFSK)
        );
        assert!(
            node_descriptor
                .mac_capabilities()
                .is_set(MacCapabilityFlag::AllocateAddress)
        );
        assert!(
            node_descriptor
                .mac_capabilities()
                .is_set(MacCapabilityFlag::SecurityCapability)
        );
        assert!(
            !node_descriptor
                .mac_capabilities()
                .is_set(MacCapabilityFlag::PowerSource)
        );
        assert_eq!(node_descriptor.manufacturer_code(), 42);
        assert_eq!(node_descriptor.maximum_buffer_size(), 8);
        assert_eq!(node_descriptor.maximum_incoming_transfer_size(), 500);
        assert!(
            node_descriptor
                .server_mask()
                .is_set(ServerMaskFlag::PrimaryTrustCenter)
        );
        assert_eq!(
            node_descriptor
                .server_mask()
                .get_stack_compliance_revision(),
            22
        );
        assert!(
            node_descriptor
                .server_mask()
                .is_set(ServerMaskFlag::BackupBindingTableCache)
        );
        assert!(
            !node_descriptor
                .server_mask()
                .is_set(ServerMaskFlag::NetworkManager)
        );
        assert_eq!(node_descriptor.maximum_outgoing_transfer_size(), 1000);
        assert!(
            node_descriptor
                .descriptor_capabilities()
                .is_set(DescriptorCapabilityFlag::ExtendedActiveEndpontListAvailable)
        );
        assert!(
            !node_descriptor
                .descriptor_capabilities()
                .is_set(DescriptorCapabilityFlag::ExtendedSimpleDescriptorListAvailable)
        );
    }
}
