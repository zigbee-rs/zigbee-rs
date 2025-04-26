pub type NwkAddress = u16;

use core::fmt;

use crate::impl_byte;
use crate::impl_pack_bytes;

impl_byte! {
    /// 16-bit network address
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub struct ShortAddress(pub u16);
}

impl fmt::Debug for ShortAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ShortAddress(0x{:04x})", self.0)
    }
}

impl Default for ShortAddress {
    fn default() -> Self {
        Self(0xffff)
    }
}

impl_byte! {
    /// 64-bit network address
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct IeeeAddress(pub u64);
}

impl fmt::Debug for IeeeAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IeeeAddress(0x{:016x})", self.0)
    }
}

impl Default for IeeeAddress {
    fn default() -> Self {
        Self(Default::default())
    }
}

pub struct MacCapabilityFlagsField(u8);

/// 2.3.2.3.6 - MAC Capability Flags Field
#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum MacCapability {
    /// The alternate PAN coordinator sub-field is one bit in length and shall
    /// be set to 1 if this node is capable of becoming a PAN coordinator.
    /// Otherwise, the alternative PAN coordinator sub-field shall be set to 0.
    AlternatePanCoordinator = 0,
    /// The device type sub-field is one bit in length and shall be set to 1 if
    /// this node is a full function device (FFD). Otherwise, the device
    /// type sub-field shall be set to 0, indicating a reduced function device
    /// (RFD).
    DeviceType = 1,
    /// The power source sub-field is one bit in length and shall be set to 1 if
    /// the current power source is mains power. Otherwise, the power source
    /// sub-field shall be set to 0. This information is derived from the
    /// node current power source field of the node power descriptor.
    PowerSource = 2,
    /// The receiver on when idle sub-field is one bit in length and shall be
    /// set to 1 if the device does not disable its receiver to conserve power
    /// during idle periods. Otherwise, the receiver on when idle sub-field
    /// shall be set to 0.
    ReceiverOnWhenIdle = 3,
    /// The security capability sub-field is one bit in length and shall be set
    /// to 1 if the device is capable of sending and receiving frames
    /// secured using the security suite specified in [B1]. Otherwise, the
    /// security capability sub-field shall be set to 0.
    SecurityCapability = 6,
    /// The allocate address sub-field is one bit in length and shall be set to
    /// 0 or 1
    AllocateAddress = 7,
}

impl MacCapabilityFlagsField {
    // Note: Capacity of IndexSet must be a power of 2.
    fn new(capabilities: FnvIndexSet<MacCapability, 8>) -> Self {
        let mut value: u8 = 0;
        for capa in capabilities.iter() {
            value |= 1 << *capa as u8
        }

        Self(value)
    }

    fn is_set(&self, capability: MacCapability) -> bool {
        return (self.0 & (1 << capability as u8)) != 0;
    }
}

/// 2.3.2.3.10 - Server Mask Field
pub struct ServerMaskField(u16);

#[repr(u8)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum ServerMaskBit {
    PrimaryTrustCenter = 0,
    BackupTrustCenter = 1,
    PrimaryBindingTableCache = 2,
    BackupBindingTableCache = 3,
    PrimaryDiscoveryCache = 4,
    BackupDiscoveryCache = 5,
    NetworkManager = 6,
}

impl ServerMaskField {
    fn new(
        server_mask_bits: FnvIndexSet<ServerMaskBit, 16>,
        stack_compliance_revision: u8,
    ) -> Self {
        let mut value: u16 = 0;
        for bit in server_mask_bits.iter() {
            value |= 1 << *bit as u16
        }

        value |= (stack_compliance_revision as u16) << 9;

        Self(value)
    }

    fn is_set(&self, server_mask_bit: ServerMaskBit) -> bool {
        return self.0 & (1 << server_mask_bit as u16) != 0;
    }

    fn get_stack_compliance_revision(&self) -> u8 {
        return (self.0 >> 9) as u8;
    }
}

pub mod macros {
    macro_rules! bitfield_bits {
        () => {
            heapless::FnvIndexSet::new()
        };
        ($ty: ty; $($x: expr),+ $(,)?) => {{
            const CAPACITY: usize =<$ty>::COUNT.next_power_of_two();
            let mut bits = heapless::FnvIndexSet::<$ty, CAPACITY>::new();
            $(
                let _ = bits.insert($x);
            )+
            bits
        }};
    }
    pub(crate) use bitfield_bits;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_mac_capabilites_should_succeed() {
        // given
        let expected: u8 = 0b1000_0001;

        // when
        let mut capas = FnvIndexSet::<MacCapability, 8>::new();
        let _ = capas.insert(MacCapability::AlternatePanCoordinator);
        let _ = capas.insert(MacCapability::AllocateAddress);
        let flagsfield = MacCapabilityFlagsField::new(capas);

        // then
        assert_eq!(expected, flagsfield.0);
    }

    #[test]
    fn reading_mac_capabilites_should_succeed() {
        // given
        let mut capas = FnvIndexSet::<MacCapability, 8>::new();
        let _ = capas.insert(MacCapability::AlternatePanCoordinator);
        let _ = capas.insert(MacCapability::AllocateAddress);

        // when
        let flagsfield = MacCapabilityFlagsField::new(capas);

        // then
        assert!(flagsfield.is_set(MacCapability::AlternatePanCoordinator));
        assert!(flagsfield.is_set(MacCapability::AllocateAddress));
        assert!(!flagsfield.is_set(MacCapability::DeviceType));
    }

    #[test]
    fn creating_server_mask_field_should_succeed() {
        // given
        let expected = 0b0010_1100_0100_0001;

        // when
        let mut bits = FnvIndexSet::<ServerMaskBit, 16>::new();
        let _ = bits.insert(ServerMaskBit::PrimaryTrustCenter);
        let _ = bits.insert(ServerMaskBit::NetworkManager);
        let server_mask_field = ServerMaskField::new(bits, 22);

        // then
        assert_eq!(expected, server_mask_field.0);
    }

    #[test]
    fn reading_server_mask_field_should_succeed() {
        // given
        let mut bits = FnvIndexSet::<ServerMaskBit, 16>::new();
        let _ = bits.insert(ServerMaskBit::PrimaryTrustCenter);
        let _ = bits.insert(ServerMaskBit::NetworkManager);

        // when
        let server_mask_field = ServerMaskField::new(bits, 22);

        // then
        assert!(server_mask_field.is_set(ServerMaskBit::PrimaryTrustCenter));
        assert!(server_mask_field.is_set(ServerMaskBit::NetworkManager));
        assert!(!server_mask_field.is_set(ServerMaskBit::PrimaryDiscoveryCache));
        assert_eq!(22, server_mask_field.get_stack_compliance_revision());
    }
}
