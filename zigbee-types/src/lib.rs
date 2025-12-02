#![no_std]

use core::fmt;
use core::mem::size_of_val;
use core::ops::Deref;
use core::ops::DerefMut;
use core::slice;

use byte::ctx;
use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use heapless::index_set::FnvIndexSet;
use heapless::Vec;
use itertools::Itertools;
use zigbee_macros::impl_byte;

pub mod storage;

pub type NwkAddress = u16;

/// A fixed-size byte array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteArray<const N: usize>(pub [u8; N]);

impl<const N: usize> Deref for ByteArray<N> {
    type Target = [u8; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, const N: usize, C: Default> TryRead<'a, C> for ByteArray<N> {
    fn try_read(bytes: &'a [u8], _: C) -> Result<(Self, usize), byte::Error> {
        let offset = &mut 0;
        let mut buf = [0u8; N];
        let data = bytes.read_with(offset, ctx::Bytes::Len(N))?;
        buf.copy_from_slice(data);
        Ok((Self(buf), *offset))
    }
}

impl<const N: usize, C: Default> TryWrite<C> for ByteArray<N> {
    fn try_write(self, bytes: &mut [u8], _: C) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        bytes.write_with(offset, &self.0[..], ())?;
        Ok(*offset)
    }
}

pub struct ByteArrayRef<'a>(pub &'a [u8]);

impl<'a> Deref for ByteArrayRef<'a> {
    type Target = &'a [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, C: Default> TryRead<'a, C> for ByteArrayRef<'a> {
    fn try_read(bytes: &'a [u8], _: C) -> Result<(Self, usize), byte::Error> {
        let offset = &mut 0;
        let data = bytes.read_with(offset, ctx::Bytes::Len(bytes.len()))?;
        Ok((Self(data), *offset))
    }
}

impl<C: Default> TryWrite<C> for ByteArrayRef<'_> {
    fn try_write(self, bytes: &mut [u8], _: C) -> Result<usize, byte::Error> {
        bytes.copy_from_slice(self.0);
        Ok(self.0.len())
    }
}

pub enum TypeArrayCtx {
    Len(usize),
}

/// A reference to a typed array.
///
/// **SAFETY**: T must be unaligned, i.e. for structs, have `#[repr(packed)]`
#[derive(Debug, Clone, Copy)]
pub struct TypeArrayRef<'a, T>(pub &'a [T]);

impl<'a, T> Deref for TypeArrayRef<'a, T> {
    type Target = &'a [T];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, C: Default> TryWrite<C> for TypeArrayRef<'_, T> {
    fn try_write(self, bytes: &mut [u8], _: C) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        let len = size_of_val(self.0);
        // SAFETY: T needs to be packed
        let buf = unsafe { slice::from_raw_parts(self.0.as_ptr().cast::<u8>(), len) };
        bytes.write_with(offset, buf, ())?;
        Ok(*offset)
    }
}

impl<'a, T> TryRead<'a, TypeArrayCtx> for TypeArrayRef<'a, T> {
    fn try_read(
        bytes: &'a [u8],
        TypeArrayCtx::Len(ctx): TypeArrayCtx,
    ) -> Result<(Self, usize), byte::Error> {
        let offset = &mut 0;
        let len = size_of::<T>() * ctx;
        let data: &[u8] = bytes.read_with(offset, ctx::Bytes::Len(len))?;
        // SAFETY: T needs to be packed
        let data: &[T] = unsafe { slice::from_raw_parts(data.as_ptr().cast::<T>(), ctx) };
        Ok((Self(data), *offset))
    }
}

#[derive(Debug)]
pub struct StorageVec<T, const N: usize>(pub Vec<T, N>);

impl<const N: usize, T: fmt::Debug> StorageVec<T, N> {
    pub fn find_or_insert_with_mut(&mut self, f: impl Fn(&T) -> bool, i: impl Fn() -> T) -> &mut T {
        let index = if let Some((i, _)) = self.iter().find_position(|v| f(v)) {
            i
        } else {
            self.push(i()).unwrap();
            self.len() - 1
        };

        self.0.get_mut(index).unwrap()
    }
}

impl<const N: usize, T> Default for StorageVec<T, N> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<const N: usize, T> StorageVec<T, N> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl<const N: usize, T> Deref for StorageVec<T, N> {
    type Target = Vec<T, N>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize, T> DerefMut for StorageVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, const N: usize, C, T> TryRead<'a, C> for StorageVec<T, N>
where
    C: Default + Copy + Clone,
    T: TryRead<'a, C>,
{
    fn try_read(bytes: &'a [u8], ctx: C) -> Result<(Self, usize), byte::Error> {
        let offset = &mut 0;
        // first 2 bytes is the length, should be enough
        let len: u16 = bytes.read_with(offset, byte::LE)?;

        let mut data: Vec<T, N> = Vec::new();
        for _i in 0..len {
            let entry: T = bytes.read_with(offset, ctx)?;
            let _ = data.push(entry);
        }
        Ok((Self(data), *offset))
    }
}

impl<const N: usize, C, T> TryWrite<C> for StorageVec<T, N>
where
    C: Default + Copy + Clone,
    T: TryWrite<C>,
{
    #[allow(clippy::cast_possible_truncation)]
    fn try_write(self, bytes: &mut [u8], ctx: C) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        // first 2 bytes is the length
        bytes.write_with(offset, self.0.len() as u16, byte::LE)?;
        for entry in self.0 {
            bytes.write_with(offset, entry, ctx)?;
        }
        Ok(*offset)
    }
}

impl_byte! {
    /// 16-bit network address
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub struct ShortAddress(pub u16);
}

impl From<u16> for ShortAddress {
    fn from(value: u16) -> Self {
        Self(value)
    }
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
    #[derive(Clone, Default, Copy, PartialEq, Eq)]
    pub struct IeeeAddress(pub u64);
}

impl From<u64> for IeeeAddress {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Debug for IeeeAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IeeeAddress(0x{:016x})", self.0)
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
    pub fn new(capabilities: &FnvIndexSet<MacCapability, 8>) -> Self {
        let mut value: u8 = 0;
        for capa in capabilities.iter() {
            value |= 1 << *capa as u8;
        }

        Self(value)
    }

    pub fn is_set(&self, capability: MacCapability) -> bool {
        (self.0 & (1 << capability as u8)) != 0
    }
}

/// 2.3.2.3.10 - Server Mask Field
pub struct ServerMaskField(pub u16);

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
    pub fn new(
        server_mask_bits: &FnvIndexSet<ServerMaskBit, 16>,
        stack_compliance_revision: u8,
    ) -> Self {
        let mut value: u16 = 0;
        for bit in server_mask_bits.iter() {
            value |= 1 << *bit as u16;
        }

        value |= (stack_compliance_revision as u16) << 9;

        Self(value)
    }

    pub fn is_set(&self, server_mask_bit: ServerMaskBit) -> bool {
        self.0 & (1 << server_mask_bit as u16) != 0
    }

    pub fn get_stack_compliance_revision(&self) -> u8 {
        (self.0 >> 9) as u8
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use super::*;

    #[test]
    fn creating_mac_capabilites_should_succeed() {
        // given
        let expected: u8 = 0b1000_0001;

        // when
        let mut capas = FnvIndexSet::<MacCapability, 8>::new();
        let _ = capas.insert(MacCapability::AlternatePanCoordinator);
        let _ = capas.insert(MacCapability::AllocateAddress);
        let flagsfield = MacCapabilityFlagsField::new(&capas);

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
        let flagsfield = MacCapabilityFlagsField::new(&capas);

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
        let server_mask_field = ServerMaskField::new(&bits, 22);

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
        let server_mask_field = ServerMaskField::new(&bits, 22);

        // then
        assert!(server_mask_field.is_set(ServerMaskBit::PrimaryTrustCenter));
        assert!(server_mask_field.is_set(ServerMaskBit::NetworkManager));
        assert!(!server_mask_field.is_set(ServerMaskBit::PrimaryDiscoveryCache));
        assert_eq!(22, server_mask_field.get_stack_compliance_revision());
    }

    #[test]
    fn bytearray_try_read_should_succeed() {
        // given
        let input_data = [0x01, 0x02, 0x03, 0x04, 0x05];

        // when
        let (result, bytes_read) = ByteArray::<5>::try_read(&input_data, ()).unwrap();

        // then
        assert_eq!(result, ByteArray(input_data));
        assert_eq!(bytes_read, 5);
    }

    #[test]
    fn bytearray_try_write_should_succeed() {
        // given
        let byte_array = ByteArray([0xaa, 0xbb, 0xcc, 0xdd]);
        let mut output_buffer = [0u8; 4];

        // when
        let bytes_written = byte_array.try_write(&mut output_buffer, ()).unwrap();

        // then
        assert_eq!(bytes_written, 4);
        assert_eq!(output_buffer, byte_array.0);
    }

    #[test]
    fn bytearrayref_try_read_should_succeed() {
        // given
        let input_data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

        // when
        let (result, bytes_read) = ByteArrayRef::try_read(&input_data, ()).unwrap();

        // then
        assert_eq!(bytes_read, 6);
        assert_eq!(result.0, input_data);
    }

    #[test]
    fn bytearrayref_try_write_should_succeed() {
        // given
        let input_data = [0x77, 0x88, 0x99];
        let byte_array_ref = ByteArrayRef(&input_data);
        let mut output_buffer = [0u8; 3];

        // when
        let bytes_written = byte_array_ref.try_write(&mut output_buffer, ()).unwrap();

        // then
        assert_eq!(bytes_written, 3);
        assert_eq!(&output_buffer[..], input_data);
    }

    #[repr(C, packed)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct TestPackedStruct {
        field1: u8,
        field2: u16,
        field3: u8,
    }

    #[test]
    fn typearrayref_try_read_should_succeed() {
        // given
        // Packed struct layout: field1(1) + field2(2) + field3(1) = 4 bytes
        let input_data = [
            0x12, 0x34, 0x56, 0x78, // field1=0x12, field2=0x5634, field3=0x78
            0xaa, 0xbb, 0xcc, 0xdd, // field1=0xaa, field2=0xccbb, field3=0xdd
        ];
        let expected_structs = [
            TestPackedStruct {
                field1: 0x12,
                field2: 0x5634,
                field3: 0x78,
            },
            TestPackedStruct {
                field1: 0xaa,
                field2: 0xccbb,
                field3: 0xdd,
            },
        ];

        // when
        let (result, bytes_read) =
            TypeArrayRef::<TestPackedStruct>::try_read(&input_data, TypeArrayCtx::Len(2)).unwrap();

        // then
        assert_eq!(bytes_read, 8);
        assert_eq!(result.0, &expected_structs);
    }

    #[test]
    fn typearrayref_try_write_should_succeed() {
        // given
        let test_structs = [
            TestPackedStruct {
                field1: 0xab,
                field2: 0xcdef,
                field3: 0x12,
            },
            TestPackedStruct {
                field1: 0x34,
                field2: 0x5678,
                field3: 0x9a,
            },
        ];
        let type_array_ref = TypeArrayRef(&test_structs);
        let mut output_buffer = [0u8; 8];

        // when
        let bytes_written = type_array_ref.try_write(&mut output_buffer, ()).unwrap();

        // then
        assert_eq!(bytes_written, 8);
        assert_eq!(
            output_buffer,
            [0xab, 0xef, 0xcd, 0x12, 0x34, 0x78, 0x56, 0x9a]
        );
    }
}
