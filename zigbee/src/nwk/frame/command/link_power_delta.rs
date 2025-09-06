use crate::internal::macros::impl_byte;
use crate::internal::types::ShortAddress;
use crate::internal::types::TypeArrayCtx;
use crate::internal::types::TypeArrayRef;

impl_byte! {
    /// Link Power Delta Command Frame
    #[derive(Debug, Clone)]
    pub struct LinkPowerDelta<'a> {
        pub command_options: CommandOptions,
        pub list_count: u8,
        #[ctx = TypeArrayCtx::Len(usize::from(list_count))]
        #[ctx_write = ()]
        pub power_list: TypeArrayRef<'a, DeltaEntry>,
    }
}

impl_byte! {
    #[tag(u8)]
    /// Link Power Delta Command Options
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum CommandOptions {
        Notification = 0x00,
        Request = 0x01,
        Response = 0x02,
        #[fallback = true]
        Reserved(u8),
    }
}

impl_byte! {
    /// Delta Entry
    #[derive(Debug, Clone)]
    #[repr(packed, Rust)]
    pub struct DeltaEntry {
        pub device_address: ShortAddress,
        pub delta: u8,
    }
}
