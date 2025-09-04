use crate::internal::macros::impl_byte;
use crate::internal::types::ShortAddress;
use crate::internal::types::TypeArrayCtx;
use crate::internal::types::TypeArrayRef;

impl_byte! {
    /// Link Status Command Frame
    #[derive(Debug, Clone)]
    pub struct LinkStatus<'a> {
        pub command_options: CommandOptions,
        #[ctx = TypeArrayCtx::Len(usize::from(command_options.entry_count()))]
        #[ctx_write = ()]
        pub entries: TypeArrayRef<'a, LinkStatusEntry>,
    }
}

impl_byte! {
    /// Link Status Command Options
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct CommandOptions(pub u8);
}

impl CommandOptions {
    /// Entry count
    pub fn entry_count(&self) -> u8 {
        (self.0 & mask::ENTRY_COUNT) >> offset::ENTRY_COUNT
    }

    /// Sets the Entry count
    #[must_use]
    pub fn set_entry_count(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::ENTRY_COUNT) | (value << offset::ENTRY_COUNT);
        self
    }

    /// First frame
    pub fn first_frame(&self) -> bool {
        (self.0 & mask::FIRST_FRAME) != 0
    }

    /// Sets the First frame
    #[must_use]
    pub fn set_first_frame(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::FIRST_FRAME) | (u8::from(value) << offset::FIRST_FRAME);
        self
    }

    /// Last frame
    pub fn last_frame(&self) -> bool {
        (self.0 & mask::LAST_FRAME) != 0
    }

    /// Sets the Last frame
    #[must_use]
    pub fn set_last_frame(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::LAST_FRAME) | (u8::from(value) << offset::LAST_FRAME);
        self
    }
}

mod offset {
    pub const ENTRY_COUNT: u8 = 0;
    pub const FIRST_FRAME: u8 = 4;
    pub const LAST_FRAME: u8 = 5;
}

mod mask {
    pub const ENTRY_COUNT: u8 = 0b0000_1111;
    pub const FIRST_FRAME: u8 = 0b0001_0000;
    pub const LAST_FRAME: u8 = 0b0010_0000;
}

impl_byte! {
    /// Neighbor Table Entry
    #[derive(Debug, Clone)]
    #[repr(packed)]
    pub struct LinkStatusEntry {
        pub neighbor_address: ShortAddress,
        pub link_status: u8,
    }
}
