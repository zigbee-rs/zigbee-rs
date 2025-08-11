#![allow(unused_variables)]

use embedded_storage::ReadStorage;
use embedded_storage::Storage;

pub struct InMemoryStorage<const N: usize> {
    pub buf: [u8; N],
}

impl<const N: usize> Default for InMemoryStorage<N> {
    fn default() -> Self {
        Self { buf: [0u8; N] }
    }
}

impl<const N: usize> ReadStorage for InMemoryStorage<N> {
    type Error = ();

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let size = offset + bytes.len();
        bytes.copy_from_slice(&self.buf[offset..size]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> Storage for InMemoryStorage<N> {
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        let size = offset + bytes.len();
        self.buf[offset..size].copy_from_slice(bytes);
        Ok(())
    }
}


pub const STORAGE_SIZE: usize = 15;

#[cfg(feature = "mock")]
use zigbee::nwk::nlme::MockNlmeSap;
use zigbee_base_device_behavior::BaseDeviceBehavior;

#[test]
fn test_initialization_procedure() {
    // given
    let storage = InMemoryStorage::<STORAGE_SIZE>::default();

    let mut nlme = MockNlmeSap::new();
    let config = zigbee::Config::default();
    let bdb_commisioning_capability = 0u8;
    let mut bdb = BaseDeviceBehavior::new(storage, &nlme, config, bdb_commisioning_capability);

    // when
    let result = bdb.start_initialization_procedure();

    // then
    assert!(result.is_ok());
    nlme.expect_network_discovery().times(1);
    // nlme.expect_network_formation().times(0);
}

