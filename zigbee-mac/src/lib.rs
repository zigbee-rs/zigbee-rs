#![no_std]

#[cfg(feature = "esp32c6")]
pub mod esp;

const MAX_IEEE802154_CANNELS: usize = 27;

pub trait Mlme {
    fn scan_network(&self, ty: ScanType, channels: u32, duration: u8);
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    Ed,
    Active,
    Passive,
    Orphan,
}
