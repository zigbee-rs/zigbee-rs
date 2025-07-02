use crate::internal::macros::impl_byte;
use crate::internal::types::IeeeAddress;
use crate::internal::types::ShortAddress;

pub struct RouteReply {
    pub command_options: CommandOptions,
    pub route_request_id: u8,
    pub originator_address: ShortAddress,
    pub responder_address: ShortAddress,
    pub path_cost: u8,
    pub originator_ieee_address: Option<IeeeAddress>,
    pub responder_ieee_address: Option<IeeeAddress>,
}

impl_byte! {
    /// Route Reply Command Options
    ///
    /// See Section 3.4.3.2.1.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct CommandOptions(pub u8);
}

impl core::fmt::Debug for CommandOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommandOptions")
            .field("responder_ieee", &self.responder_ieee())
            .field("originator_ieee", &self.originator_ieee())
            .field("multicast", &self.multicast())
            .finish()
    }
}

impl CommandOptions {
    /// Originator IEEE Address flag
    pub fn originator_ieee(&self) -> bool {
        (self.0 & mask::ORIGINATOR_IEEE) != 0
    }

    /// Sets the Originator IEEE Address flag
    #[must_use]
    pub fn set_originator_ieee(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::ORIGINATOR_IEEE) | (u8::from(value) << offset::ORIGINATOR_IEEE);
        self
    }

    /// Responder IEEE Address flag
    pub fn responder_ieee(&self) -> bool {
        (self.0 & mask::RESPONDER_IEEE) != 0
    }

    /// Sets the Responder IEEE Address flag
    #[must_use]
    pub fn set_responder_ieee(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::RESPONDER_IEEE) | (u8::from(value) << offset::RESPONDER_IEEE);
        self
    }

    /// Multicast flag
    pub fn multicast(&self) -> bool {
        (self.0 & mask::MULTICAST) != 0
    }

    /// Sets the Multicast flag
    #[must_use]
    pub fn set_multicast(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::MULTICAST) | (u8::from(value) << offset::MULTICAST);
        self
    }
}

mod offset {
    pub const ORIGINATOR_IEEE: u8 = 4;
    pub const RESPONDER_IEEE: u8 = 5;
    pub const MULTICAST: u8 = 6;
}

mod mask {
    pub const ORIGINATOR_IEEE: u8 = 0b0001_0000;
    pub const RESPONDER_IEEE: u8 = 0b0010_0000;
    pub const MULTICAST: u8 = 0b0100_0000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_options() {
        let mut options = CommandOptions(0);

        // Test originator IEEE flag
        assert!(!options.originator_ieee());
        assert!(!options.responder_ieee());
        assert!(!options.multicast());

        // Test all flags together
        options = options
            .set_originator_ieee(true)
            .set_responder_ieee(true)
            .set_multicast(true);
        assert!(options.originator_ieee());
        assert!(options.responder_ieee());
        assert!(options.multicast());
    }
}
