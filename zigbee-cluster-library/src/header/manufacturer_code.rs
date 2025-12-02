//! Manufacturer Code
use zigbee_macros::impl_byte;

impl_byte! {
    /// See Section 2.4.1.2
    #[derive(Clone, Debug, Copy, PartialEq, Eq)]
    pub struct ManufacturerCode(pub u16);
}
