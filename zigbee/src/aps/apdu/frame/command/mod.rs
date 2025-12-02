//use crate::impl_byte;
//
//impl_byte! {
//    #[repr(u8)]
//    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//    pub enum Command {
//        TransportKey = 0x05,
//        UpdateDevice = 0x06,
//        RemoveDevice = 0x07,
//        RequestKey = 0x08,
//        SwitchKey = 0x09,
//        Tunnel = 0x0e,
//        VerifyKey = 0x0f,
//        ConfirmKey = 0x10,
//        #[fallback = true]
//        Reserved,
//    }
//}
use zigbee_macros::impl_byte;

mod request_key;
mod transport_key;

pub use request_key::*;
pub use transport_key::*;

impl_byte! {
    // 4.4.10
    // Table 4-27
    #[tag(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Command {
        #[tag_value = 0x05]
        TransportKey(TransportKey),
        #[tag_value = 0x08]
        RequestKey(RequestKey),
        #[fallback = true]
        Reserved(u8),
    }
}
