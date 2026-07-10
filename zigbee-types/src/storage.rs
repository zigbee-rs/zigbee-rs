use embedded_storage::ReadStorage;
use embedded_storage::Storage;

pub struct InMemoryStorage<const N: usize> {
    buf: [u8; N],
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
