//! NWK Frame Header Control
use core::mem;

use crate::impl_byte;

impl_byte! {
    /// Frame Control field
    ///
    /// See Section 3.3.1.1.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct FrameControl(pub u16);
}

impl core::fmt::Debug for FrameControl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrameControl")
            .field("frame_type", &self.frame_type())
            .field("protocol_version", &self.protocol_version())
            .field("discover_route", &self.discover_route())
            .field("multicast_flag", &self.multicast_flag())
            .field("security_flag", &self.security_flag())
            .field("source_flag", &self.source_flag())
            .field("destination_ieee_flag", &self.destination_ieee_flag())
            .field("source_ieee_flag", &self.source_ieee_flag())
            .field("end_device_initiator", &self.end_device_initiator())
            .finish()
    }
}

impl FrameControl {
    /// See Section 3.3.1.1.
    pub fn frame_type(&self) -> FrameType {
        // SAFETY: any 2 bit permutation is a valid FrameType
        unsafe { mem::transmute(((self.0 & mask::FRAME_TYPE) >> offset::FRAME_TYPE) as u8) }
    }

    /// Sets the frame type
    #[must_use]
    pub fn set_frame_type(mut self, value: FrameType) -> Self {
        self.0 |= (value as u16) << offset::FRAME_TYPE;
        self
    }

    /// Protocol version
    pub fn protocol_version(&self) -> u8 {
        ((self.0 & mask::PROTOCOL) >> offset::PROTOCOL) as u8
    }

    /// Sets the protocol version
    #[must_use]
    pub fn set_protocol_version(mut self, value: u8) -> Self {
        self.0 |= u16::from(value) << offset::PROTOCOL;
        self
    }

    /// Discover Route flag
    pub fn discover_route(&self) -> DiscoverRoute {
        DiscoverRoute::from_bits(((self.0 & mask::DISCOVER_ROUTE) >> offset::DISCOVER_ROUTE) as u8)
    }

    /// Sets the Discover Route flag
    #[must_use]
    pub fn set_discover_route(mut self, value: DiscoverRoute) -> Self {
        self.0 |= (value as u16) << offset::DISCOVER_ROUTE;
        self
    }

    /// Multicast Flag
    pub fn multicast_flag(&self) -> bool {
        ((self.0 & mask::MULTICAST_FLAG) >> offset::MULTICAST_FLAG) != 0
    }

    /// Sets the Multicast Flag
    #[must_use]
    pub fn set_multicast_flag(mut self, value: bool) -> Self {
        self.0 |= u16::from(value) << offset::MULTICAST_FLAG;
        self
    }

    /// Security flag
    ///
    /// The security sub-field shall have a value of 1 if, and only if, the
    /// frame is to have NWK security operations enabled. If security for
    /// this frame is implemented at another layer or disabled entirely,
    /// it shall have a value of 0.
    pub fn security_flag(&self) -> bool {
        ((self.0 & mask::SECURITY) >> offset::SECURITY) != 0
    }

    /// Sets the Security flag
    #[must_use]
    pub fn set_security_flag(mut self, value: bool) -> Self {
        self.0 |= u16::from(value) << offset::SECURITY;
        self
    }

    /// Source Route flag
    ///
    /// The source route sub-field shall have a value of 1 if and only if a
    /// source route subframe is present in the NWK header. If the source
    /// route subframe is not present, the source route sub-field shall have
    /// a value of 0.
    pub fn source_flag(&self) -> bool {
        ((self.0 & mask::SOURCE_ROUTE) >> offset::SOURCE_ROUTE) != 0
    }

    /// Sets the Source Route flag
    #[must_use]
    pub fn set_source_flag(mut self, value: bool) -> Self {
        self.0 |= u16::from(value) << offset::SOURCE_ROUTE;
        self
    }

    /// Destination IEEE Address flag
    ///
    /// The destination IEEE address sub-field shall have a value of 1 if, and
    /// only if, the NWK header is to include the full IEEE address of the
    /// destination.
    pub fn destination_ieee_flag(&self) -> bool {
        ((self.0 & mask::DEST_IEEE_ADDR) >> offset::DEST_IEEE_ADDR) != 0
    }

    /// Sets the Destination IEEE Address flag
    #[must_use]
    pub fn set_destination_ieee_flag(mut self, value: bool) -> Self {
        self.0 |= u16::from(value) << offset::DEST_IEEE_ADDR;
        self
    }

    /// Source IEEE Address flag
    ///
    /// The source IEEE address sub-field shall have a value of 1 if, and only
    /// if, the NWK header is to include the full IEEE address of the source
    /// device.
    pub fn source_ieee_flag(&self) -> bool {
        ((self.0 & mask::SRC_IEEE_ADDR) >> offset::SRC_IEEE_ADDR) != 0
    }

    /// Sets the Source IEEE Address flag
    #[must_use]
    pub fn set_source_ieee_flag(mut self, value: bool) -> Self {
        self.0 |= u16::from(value) << offset::SRC_IEEE_ADDR;
        self
    }

    /// End Device Iterator flag
    pub fn end_device_initiator(&self) -> bool {
        ((self.0 & mask::END_DEV_ITER) >> offset::END_DEV_ITER) != 0
    }

    /// Sets the End Device Iterator flag
    #[must_use]
    pub fn set_end_device_initiator(mut self, value: bool) -> Self {
        self.0 |= u16::from(value) << offset::END_DEV_ITER;
        self
    }

    /// See Table 3-45.
    pub fn transmission_method(&self) -> DataTransmissionMethod {
        match (
            self.discover_route(),
            self.multicast_flag(),
            self.destination_ieee_flag(),
        ) {
            (DiscoverRoute::Suppress, false, false) => DataTransmissionMethod::Broadcast,
            (DiscoverRoute::Suppress, true, false) => DataTransmissionMethod::Multicast,
            (DiscoverRoute::Suppress | DiscoverRoute::Enable, false, _) => {
                DataTransmissionMethod::Unicast
            }
            //(DiscoverRoute::Suppress, false, _) => DataTransmissionMethod::SourceRouted,
            (_, _, _) => unreachable!(),
        }
    }
}

/// Data Transmission Method
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataTransmissionMethod {
    Unicast,
    Broadcast,
    Multicast,
    SourceRouted,
}

/// Frame Type
///
/// See Section 3.3.1.1.1.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Data = 0b00,
    NwkCommand = 0b01,
    Reserved = 0b10,
    InterPan = 0b11,
}

/// Discover Route
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverRoute {
    Suppress,
    Enable,
    Reserved,
}

impl DiscoverRoute {
    fn from_bits(b: u8) -> Self {
        match b {
            0x00 => Self::Suppress,
            0x01 => Self::Enable,
            _ => Self::Reserved,
        }
    }
}

mod offset {
    pub const FRAME_TYPE: u16 = 0;
    pub const PROTOCOL: u16 = 2;
    pub const DISCOVER_ROUTE: u16 = 6;
    pub const MULTICAST_FLAG: u16 = 8;
    pub const SECURITY: u16 = 9;
    pub const SOURCE_ROUTE: u16 = 10;
    pub const DEST_IEEE_ADDR: u16 = 11;
    pub const SRC_IEEE_ADDR: u16 = 12;
    pub const END_DEV_ITER: u16 = 13;
}

mod mask {
    pub const FRAME_TYPE: u16 = 0x3;
    pub const PROTOCOL: u16 = 0x3C;
    pub const DISCOVER_ROUTE: u16 = 0xC0;
    pub const MULTICAST_FLAG: u16 = 0x100;
    pub const SECURITY: u16 = 0x200;
    pub const SOURCE_ROUTE: u16 = 0x400;
    pub const DEST_IEEE_ADDR: u16 = 0x800;
    pub const SRC_IEEE_ADDR: u16 = 0x1000;
    pub const END_DEV_ITER: u16 = 0x2000;
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;

    #[test]
    fn parse_frame_control() {
        let raw = [0b0111_1100_u8, 0b0010_1010_u8];

        let (frame_control, len) = FrameControl::try_read(&raw, ()).unwrap();
        assert_eq!(len, 2);
        assert_eq!(frame_control.frame_type(), FrameType::Data);
        assert_eq!(frame_control.protocol_version(), 0b1111u8);
        assert_eq!(frame_control.discover_route(), DiscoverRoute::Enable);
        assert!(!frame_control.multicast_flag());
        assert!(frame_control.security_flag());
        assert!(!frame_control.source_flag());
        assert!(frame_control.destination_ieee_flag());
        assert!(!frame_control.source_ieee_flag());
        assert!(frame_control.end_device_initiator());
    }
}
