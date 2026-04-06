use zigbee_macros::impl_byte;

mod confirm_key;
mod request_key;
mod transport_key;
mod verify_key;

pub use confirm_key::*;
pub use request_key::*;
pub use transport_key::*;
pub use verify_key::*;

impl_byte! {
    // §4.4.10, Table 4-27
    #[tag(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Command {
        #[tag_value = 0x05]
        TransportKey(TransportKey),
        #[tag_value = 0x08]
        RequestKey(RequestKey),
        #[tag_value = 0x0f]
        VerifyKey(VerifyKey),
        #[tag_value = 0x10]
        ConfirmKey(ConfirmKey),
        #[fallback = true]
        Reserved(u8),
    }
}
