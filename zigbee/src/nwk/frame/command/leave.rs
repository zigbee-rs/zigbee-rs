use crate::internal::macros::impl_byte;

impl_byte! {
    /// Leave Command Frame
    #[derive(Debug, Clone)]
    pub struct Leave {
        pub command_options: CommandOptions,
    }
}

impl_byte! {
    /// Leave Command Options
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct CommandOptions(pub u8);
}

impl core::fmt::Debug for CommandOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommandOptions")
            .field("rejoin", &self.rejoin())
            .field("request", &self.request())
            .field("remove_children", &self.remove_children())
            .finish()
    }
}

impl CommandOptions {
    /// Rejoin flag
    pub fn rejoin(&self) -> bool {
        (self.0 & mask::REJOIN) != 0
    }

    /// Sets the Rejoin flag
    #[must_use]
    pub fn set_rejoin(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::REJOIN) | (u8::from(value) << offset::REJOIN);
        self
    }

    /// Request flag
    pub fn request(&self) -> bool {
        (self.0 & mask::REQUEST) != 0
    }

    /// Sets the Request flag
    #[must_use]
    pub fn set_request(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::REQUEST) | (u8::from(value) << offset::REQUEST);
        self
    }

    /// Remove children flag
    pub fn remove_children(&self) -> bool {
        (self.0 & mask::REMOVE_CHILDREN) != 0
    }

    /// Sets the Remove children flag
    #[must_use]
    pub fn set_remove_children(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::REMOVE_CHILDREN) | (u8::from(value) << offset::REMOVE_CHILDREN);
        self
    }
}

mod offset {
    pub const REJOIN: u8 = 5;
    pub const REQUEST: u8 = 6;
    pub const REMOVE_CHILDREN: u8 = 7;
}

mod mask {
    pub const REJOIN: u8 = 0b0010_0000;
    pub const REQUEST: u8 = 0b0100_0000;
    pub const REMOVE_CHILDREN: u8 = 0b1000_0000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_options() {
        let mut options = CommandOptions(0);
        assert!(!options.rejoin());
        assert!(!options.request());
        assert!(!options.remove_children());

        options = options
            .set_rejoin(true)
            .set_request(true)
            .set_remove_children(true);
        assert!(options.rejoin());
        assert!(options.request());
        assert!(options.remove_children());
    }
}
