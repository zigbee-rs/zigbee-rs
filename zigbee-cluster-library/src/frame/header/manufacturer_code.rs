//! Manufacturer Code
use zigbee_macros::impl_byte;

impl_byte! {
    /// See Section 2.4.1.2
    #[repr(transparent)]
    #[derive(Clone, Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct ManufacturerCode(pub u16);
}

impl ManufacturerCode {
    pub const fn new(val: u16) -> Self {
        Self(val)
    }
}
