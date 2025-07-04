//! NWK Frame Header
use super::frame_control::FrameControl;
use crate::internal::macros::impl_byte;
use crate::internal::types::IeeeAddress;
use crate::internal::types::ShortAddress;

impl_byte! {
    /// 3.3.1 General NPDU Frame Format
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Header<'a> {
        /// See Section 3.3.1.1.
        pub frame_control: FrameControl,
        /// See Section 3.3.1.2.
        pub destination: ShortAddress,
        /// See Section 3.3.1.3.
        pub source: ShortAddress,
        /// See Section 3.3.1.4.
        pub radius: u8,
        /// See Section 3.3.1.5.
        pub sequence_number: u8,
        /// Set only if [`FrameControl::destination_ieee_flag`] is `true`.
        /// See Section 3.3.1.6.
        #[parse_if = frame_control.destination_ieee_flag()]
        pub destination_ieee: Option<IeeeAddress>,
        /// Set only if [`FrameControl::source_ieee_flag`] is `true`.
        /// See Section 3.3.1.7.
        #[parse_if = frame_control.source_ieee_flag()]
        pub source_ieee: Option<IeeeAddress>,
        /// Set only if [`FrameControl::multicast_flag`] is `true`.
        /// See Section 3.3.1.8.
        #[parse_if = frame_control.multicast_flag()]
        pub multicast_control: Option<MulticastControl>,
        /// Set only if [`FrameControl::source_flag`] is `true`.
        /// See Section 3.3.1.9.
        #[parse_if = frame_control.source_flag()]
        pub source_route_subframe: Option<SourceRouteSubframe<'a>>,
    }
}

impl_byte! {
    /// 3.3.1.8 Multicast Control Field
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MulticastControl(u8);
}

impl_byte! {
    /// Source Route Subframe
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SourceRouteSubframe<'a> {
        /// Indicates the number of relays contained in
        /// [`SourceRouteSubframe::relay_list`].
        ///
        /// See Section 3.3.1.9.1.
        pub relay_count: u8,
        /// Indicates the index of the next relay in
        /// [`SourceRouteSubframe::relay_list`] to which the packet will be
        /// transmitted.
        ///
        /// See Section 3.3.1.9.2.
        pub relay_index: u8,
        /// List of relay addresses from closest to the destination to closest to
        /// the originator.
        ///
        /// See Section 3.3.1.9.2.
        #[ctx = byte::ctx::Bytes::Len(relay_count as usize)]
        #[ctx_write = ()]
        pub relay_list: &'a [u8],
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;

    #[test]
    fn parse_nwk_header() {
        let raw = [
            0x09, 0x12, 0xfc, 0xff, 0x00, 0x00, 0x08, 0xbf, 0x66, 0x71, 0x9a, 0x2a, 0x00, 0x4b,
            0x12, 0x00,
        ];

        let (header, _) = Header::try_read(&raw, ()).unwrap();

        assert!(header.frame_control.security_flag());
        assert!(header.frame_control.source_ieee_flag());
        assert_eq!(header.destination, ShortAddress(0xfffc));
        assert_eq!(header.source_ieee, Some(IeeeAddress(0x0012_4b00_2a9a_7166)));
        assert_eq!(header.radius, 8);
        assert_eq!(header.sequence_number, 191);
    }
}
