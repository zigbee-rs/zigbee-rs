use crate::internal::macros::impl_byte;
use crate::internal::types::IeeeAddress;
use crate::internal::types::ShortAddress;

pub struct RouteRequest {
    pub command_options: CommandOptions,
    pub route_request_id: u8,
    pub destination_address: ShortAddress,
    pub path_cost: u8,
    pub destination_ieee_address: Option<IeeeAddress>,
}

impl_byte! {
    /// Route Request Command Options
    ///
    /// See Section 3.4.3.1.1.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct CommandOptions(pub u8);
}

impl core::fmt::Debug for CommandOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommandOptions")
            .field("many_to_one", &self.many_to_one())
            .field("destination_ieee", &self.destination_ieee())
            .finish()
    }
}

impl CommandOptions {
    /// Many-to-one flag
    ///
    /// The many-to-one flag shall be set to 1 if the route request is being
    /// initiated by a concentrator device. Otherwise, it shall be set to 0.
    pub fn many_to_one(&self) -> u8 {
        (self.0 & mask::MANY_TO_ONE) >> offset::MANY_TO_ONE
    }

    /// Sets the Many-to-one flag
    #[must_use]
    pub fn set_many_to_one(mut self, value: u8) -> Self {
        self.0 = (self.0 & !mask::MANY_TO_ONE) | (value << offset::MANY_TO_ONE);
        self
    }

    /// Destination IEEE Address flag
    ///
    /// The destination IEEE address flag shall be set to 1 if the destination
    /// IEEE address field is present. Otherwise, it shall be set to 0.
    pub fn destination_ieee(&self) -> bool {
        (self.0 & mask::DEST_IEEE) != 0
    }

    /// Sets the Destination IEEE Address flag
    #[must_use]
    pub fn set_destination_ieee(mut self, value: bool) -> Self {
        self.0 = (self.0 & !mask::DEST_IEEE) | (u8::from(value) << offset::DEST_IEEE);
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
    pub const MANY_TO_ONE: u8 = 3;
    pub const DEST_IEEE: u8 = 5;
    pub const MULTICAST: u8 = 6;
}

mod mask {
    pub const MANY_TO_ONE: u8 = 0b0001_1000;
    pub const DEST_IEEE: u8 = 0b0010_0000;
    pub const MULTICAST: u8 = 0b0100_0000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_options() {
        let options = CommandOptions(0b0110_1000);

        assert_eq!(options.many_to_one(), 1);
        assert!(options.destination_ieee());
        assert!(options.multicast());

        let options = options
            .set_many_to_one(255)
            .set_destination_ieee(false)
            .set_multicast(false);

        assert_eq!(options.many_to_one(), 3);
        assert!(!options.destination_ieee());
        assert!(!options.multicast());
    }
}
