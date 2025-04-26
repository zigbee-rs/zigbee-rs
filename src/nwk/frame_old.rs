//! NWK Frame Formats
use core::fmt::Debug;
use core::mem::{self};

use heapless::Vec;

use crate::common::parse::PackBytes;
use crate::common::types::IeeeAddress;
use crate::common::types::ShortAddress;
use crate::impl_pack_bytes;

/// 3.5.1.
#[allow(dead_code)]
const PROTOCOL_VERSION: u8 = 0x02;

const PAYLOAD_SIZE: usize = 128;

#[derive(Debug)]
#[allow(missing_docs)]
pub enum NwkFrame {
    Data(NwkDataFrame),
    NwkCommand(NwkCommandFrame),
    Reserved(NwkHeader),
    InterPan(NwkHeader),
}

impl NwkFrame {
    /// Return the [`FrameTypeIdentifier`] of a [`NwkFrame`].
    pub fn frame_type_identifier(&self) -> FrameTypeIdentifier {
        match self {
            Self::Data(nwk_data_frame) => {
                nwk_data_frame.header.frame_control.frame_type_identifier()
            }
            Self::NwkCommand(nwk_command_frame) => nwk_command_frame
                .header
                .frame_control
                .frame_type_identifier(),
            Self::Reserved(nwk_header) | Self::InterPan(nwk_header) => {
                nwk_header.frame_control.frame_type_identifier()
            }
        }
    }
}

impl PackBytes for NwkFrame {
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        let mut src = src.into_iter();
        let header = NwkHeader::unpack_from_iter(&mut src)?;
        let frame = match header.frame_control.frame_type_identifier() {
            FrameTypeIdentifier::Data => Self::Data(NwkDataFrame {
                header,
                payload: PackBytes::unpack_from_iter(&mut src)?,
            }),
            FrameTypeIdentifier::NwkCommand => Self::NwkCommand(NwkCommandFrame {
                header,
                command_identifier: PackBytes::unpack_from_iter(&mut src)?,
                payload: PackBytes::unpack_from_iter(&mut src)?,
            }),
            FrameTypeIdentifier::Reserved => Self::Reserved(header),
            FrameTypeIdentifier::InterPan => Self::InterPan(header),
        };
        Some(frame)
    }
}

impl_pack_bytes! {
    /// NWK Date Frame
    ///
    /// See Section 3.3.2.1.
    #[derive(Debug)]
    #[allow(missing_docs)]
    pub struct NwkDataFrame {
        #[pack = true]
        pub header: NwkHeader,
        #[pack = true]
        pub payload: Vec<u8, PAYLOAD_SIZE>,
    }
}

impl_pack_bytes! {
    /// NWK Command Frame.
    ///
    /// See Section 3.3.2.2.
    #[derive(Debug)]
    #[allow(missing_docs)]
    pub struct NwkCommandFrame {
        #[pack = true]
        pub header: NwkHeader,
        #[pack = true]
        pub command_identifier: CommandFrameIdentifier,
        #[pack = true]
        pub payload: Vec<u8, PAYLOAD_SIZE>,
    }
}

/// Comand Frame Identifiers.
///
/// See Section 3.4.
//#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum CommandFrameIdentifier {
    RouteRequest = 0x01,
    RouteReply = 0x02,
    NetworkStatus = 0x03,
    Leave = 0x04,
    RouteRecord = 0x05,
    RejoinRequest = 0x06,
    RejoinResponse = 0x07,
    LinkStatus = 0x08,
    NetworkReport = 0x09,
    NetworkUpdate = 0x0a,
    EndDeviceTimeoutRequest = 0x0b,
    EndDeviceTimeoutResponse = 0x0c,
    LinkPowerDelta = 0x0d,
    Reserved,
}

impl PackBytes for CommandFrameIdentifier {
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        let b = src.into_iter().next()?;
        if b <= 0x0d {
            // SAFETY: any byte with value <= 0x0d is a valid CommandFrameIdentifier
            Some(unsafe { mem::transmute::<u8, Self>(b) })
        } else {
            Some(Self::Reserved)
        }
    }
}

impl_pack_bytes! {
    /// 3.3.1 General NPDU Frame Format
    #[derive(Debug)]
    pub struct NwkHeader {
        /// See Section 3.3.1.1.
        #[control_header = FrameControl]
        pub frame_control: FrameControl,
        /// See Section 3.3.1.2.
        #[pack = true]
        pub destination: ShortAddress,
        /// See Section 3.3.1.3.
        #[pack = true]
        pub source: ShortAddress,
        /// See Section 3.3.1.4.
        #[pack = true]
        pub radius: u8,
        /// See Section 3.3.1.5.
        #[pack = true]
        pub sequence_number: u8,
        /// Set only if [`FrameControl::destination_ieee_flag`] is `true`.
        /// See Section 3.3.1.6.
        #[pack_if = FrameControl::destination_ieee_flag]
        pub destination_ieee: Option<IeeeAddress>,
        /// Set only if [`FrameControl::source_ieee_flag`] is `true`.
        /// See Section 3.3.1.7.
        #[pack_if = FrameControl::source_ieee_flag]
        pub source_ieee: Option<IeeeAddress>,
        /// Set only if [`FrameControl::multicast_flag`] is `true`.
        /// See Section 3.3.1.8.
        #[pack_if = FrameControl::multicast_flag]
        pub multicast_control: Option<MulticastControl>,
        /// Set only if [`FrameControl::source_flag`] is `true`.
        /// See Section 3.3.1.9.
        #[pack_if = FrameControl::source_flag]
        pub source_route_subframe: Option<SourceRouteSubframe>,
    }
}

impl_pack_bytes! {
    /// 3.3.1.1 Frame Control Field
    #[derive(Clone, Copy)]
    pub struct FrameControl(pub u16);
}

impl Debug for FrameControl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrameControl")
            .field("frame_type", &self.frame_type_identifier())
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
    pub fn frame_type_identifier(&self) -> FrameTypeIdentifier {
        // SAFETY: any 2 bit permutation is a valid FrameType
        unsafe { mem::transmute((self.0 & 0b11) as u8) }
    }

    /// See Section 3.3.1.1.2.
    pub fn protocol_version(&self) -> u8 {
        ((self.0 >> 2) & 0b1111) as u8
    }

    /// See Section 3.3.1.1.3.
    pub fn discover_route(&self) -> DiscoverRoute {
        DiscoverRoute::from_u8(((self.0 >> 6) & 0b11) as u8)
    }

    /// See Section 3.3.1.1.4.
    pub fn multicast_flag(&self) -> bool {
        ((self.0 >> 8) & 0b1) != 0
    }

    /// The security sub-field shall have a value of 1 if, and only if, the
    /// frame is to have NWK security operations enabled. If security for
    /// this frame is implemented at another layer or disabled entirely,
    /// it shall have a value of 0.
    ///
    /// See Section 3.3.1.1.5.
    pub fn security_flag(&self) -> bool {
        ((self.0 >> 9) & 0b1) != 0
    }

    /// The source route sub-field shall have a value of 1 if and only if a
    /// source route subframe is present in the NWK header. If the source
    /// route subframe is not present, the source route sub-field shall have
    /// a value of 0.
    ///
    /// See Section 3.3.1.1.6.
    pub fn source_flag(&self) -> bool {
        ((self.0 >> 10) & 0b1) != 0
    }

    /// The destination IEEE address sub-field shall have a value of 1 if, and
    /// only if, the NWK header is to include the full IEEE address of the
    /// destination.
    ///
    /// See Section 3.3.1.1.7.
    pub fn destination_ieee_flag(&self) -> bool {
        ((self.0 >> 11) & 0b1) != 0
    }

    /// The source IEEE address sub-field shall have a value of 1 if, and only
    /// if, the NWK header is to include the full IEEE address of the source
    /// device.
    ///
    /// See Section 3.3.1.1.8.
    pub fn source_ieee_flag(&self) -> bool {
        ((self.0 >> 12) & 0b1) != 0
    }

    /// See Section 3.3.1.1.9.
    pub fn end_device_initiator(&self) -> bool {
        ((self.0 >> 13) & 0b1) != 0
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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DataTransmissionMethod {
    Unicast,
    Broadcast,
    Multicast,
    SourceRouted,
}

/// 3.3.1.1.1 Frame Type Sub-Field
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum FrameTypeIdentifier {
    Data = 0b00,
    NwkCommand = 0b01,
    Reserved = 0b10,
    InterPan = 0b11,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DiscoverRoute {
    Suppress,
    Enable,
    Reserved,
}

impl DiscoverRoute {
    fn from_u8(b: u8) -> Self {
        match b {
            0x00 => Self::Suppress,
            0x01 => Self::Enable,
            _ => Self::Reserved,
        }
    }
}

impl_pack_bytes! {
    /// 3.3.1.8 Multicast Control Field
    pub struct MulticastControl(u8);
}

impl Debug for MulticastControl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MulticastControl")
            .field("multicast_mode", &self.multicast_mode())
            .field("non_member_radius", &self.non_member_radius())
            .field("max_member_radius", &self.max_member_radius())
            .finish()
    }
}

impl MulticastControl {
    /// See Section 3.3.1.8.1.
    pub fn multicast_mode(&self) -> MulticastMode {
        match (self.0 >> 6) & 0b11 {
            0b00 => MulticastMode::NonMemberMode,
            0b01 => MulticastMode::MemberMode,
            _ => MulticastMode::Reserved,
        }
    }

    /// See Section 3.3.1.8.2.
    pub fn non_member_radius(&self) -> u8 {
        (self.0 >> 3) & 0b111
    }

    /// The maximum value of the [`MulticastControl::non_member_radius`]
    /// sub-field for this frame. See Section 3.3.1.8.3.
    pub fn max_member_radius(&self) -> u8 {
        self.0 & 0b11
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum MulticastMode {
    NonMemberMode = 0b00,
    MemberMode = 0b01,
    Reserved,
}

const RELAY_LIST_SIZE: usize = 16;

impl_pack_bytes! {
    /// Source Route Subframe
    #[derive(Debug)]
    pub struct SourceRouteSubframe {
        /// Indicates the number of relays contained in [`SourceRouteSubframe::relay_list`].
        ///
        /// See Section 3.3.1.9.1.
        #[pack = true]
        pub relay_count: u8,
        /// Indicates the index of the next relay in [`SourceRouteSubframe::relay_list`] to
        /// which the packet will be transmitted.
        ///
        /// See Section 3.3.1.9.2.
        #[pack = true]
        pub relay_index: u8,
        /// List of relay addresses from closest to the destination to closest to the originator.
        ///
        /// See Section 3.3.1.9.2.
        #[pack = true]
        pub relay_list: Vec<u8, RELAY_LIST_SIZE>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    //const CMD_FRAME: &str =
    //"0912fcff000008bf66719a2a004b120028e6ff3d001bc928c67f38c1a4008cf1882da4b1bbfcb9be";

    #[test]
    fn multicast_control_multicast_mode() {
        let multicast_control = MulticastControl(0xff);
        let mode = multicast_control.multicast_mode();
        assert_eq!(mode, MulticastMode::Reserved);
    }

    #[test]
    fn source_route_subframe_pack() {
        let got =
            SourceRouteSubframe::unpack_from_slice(&[0x04, 0x03, 0xff, 0xff, 0xff, 0xff]).unwrap();

        assert_eq!(got.relay_count, 0x04);
        assert_eq!(got.relay_index, 0x03);
        assert_eq!(got.relay_list, &[0xff, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn unpack_frame_control() {
        let raw = [0b0111_1100_u8, 0b0010_1010_u8];

        let frame_control = FrameControl::unpack_from_slice(&raw).unwrap();
        assert_eq!(
            frame_control.frame_type_identifier(),
            FrameTypeIdentifier::Data
        );
        assert_eq!(frame_control.protocol_version(), 0b1111u8);
        assert_eq!(frame_control.discover_route(), DiscoverRoute::Enable);
        assert!(!frame_control.multicast_flag());
        assert!(frame_control.security_flag());
        assert!(!frame_control.source_flag());
        assert!(frame_control.destination_ieee_flag());
        assert!(!frame_control.source_ieee_flag());
        assert!(frame_control.end_device_initiator());
    }

    #[test]
    fn unpack_nwk_header() {
        let raw = [
            0x09, 0x12, 0xfc, 0xff, 0x00, 0x00, 0x08, 0xbf, 0x66, 0x71, 0x9a, 0x2a, 0x00, 0x4b,
            0x12, 0x00,
        ];

        let header = NwkHeader::unpack_from_slice(&raw).unwrap();

        assert!(header.frame_control.security_flag());
        assert!(header.frame_control.source_ieee_flag());
        assert_eq!(header.destination, ShortAddress(0xfffc));
        assert_eq!(header.source_ieee, Some(IeeeAddress(0x0012_4b00_2a9a_7166)));
        assert_eq!(header.radius, 8);
        assert_eq!(header.sequence_number, 191);
    }

    //#[test]
    //fn unpack_command_frame_identifier() {
    //    let raw = [0b00000111u8];

    //    let id = CommandFrameIdentifier::unpack_from_slice(&raw).unwrap();

    //    assert_eq!(id, CommandFrameIdentifier::RejoinResponse)
    //}
}
