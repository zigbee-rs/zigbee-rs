#![no_std]

#[cfg(feature = "esp32c6")]
pub mod esp;

const MAX_IEEE802154_CHANNELS: u8 = 27;

const A_BASE_SLOT_DURATION: u32 = 60;
const A_NUM_SUPER_FRAME_SLOTS: u32 = 16;
const A_BASE_SUPER_FRAME_DURATION: u32 = A_BASE_SLOT_DURATION * A_NUM_SUPER_FRAME_SLOTS;

pub enum MacError {}

pub trait Mlme {
    fn scan_network(&mut self, ty: ScanType, channels: u32, duration: u8) -> Result<(), MacError>;
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    Ed,
    Active,
    Passive,
    Orphan,
}

#[derive(Debug)]
pub struct ScanResult {
    pub channel: u8,
    // ..
}
