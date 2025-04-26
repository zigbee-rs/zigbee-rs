use byte::BytesExt;
use byte::TryRead;

use super::frame_control::FrameControl;
use crate::common::types::IeeeAddress;
use crate::common::types::ShortAddress;
use crate::impl_byte;

/// 3.3.1 General NPDU Frame Format
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NwkHeader<'a> {
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
    pub destination_ieee: Option<IeeeAddress>,
    /// Set only if [`FrameControl::source_ieee_flag`] is `true`.
    /// See Section 3.3.1.7.
    pub source_ieee: Option<IeeeAddress>,
    /// Set only if [`FrameControl::multicast_flag`] is `true`.
    /// See Section 3.3.1.8.
    pub multicast_control: Option<MulticastControl>,
    /// Set only if [`FrameControl::source_flag`] is `true`.
    /// See Section 3.3.1.9.
    pub source_route_subframe: Option<SourceRouteSubframe<'a>>,
}

impl<'a> TryRead<'a> for NwkHeader<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let frame_control: FrameControl = bytes.read_with(offset, byte::BE)?;

        let destination = bytes.read(offset)?;
        let source = bytes.read(offset)?;
        let radius = bytes.read(offset)?;
        let sequence_number = bytes.read(offset)?;

        let destination_ieee = if frame_control.destination_ieee_flag() {
            Some(bytes.read(offset)?)
        } else {
            None
        };
        let source_ieee = if frame_control.source_ieee_flag() {
            Some(bytes.read(offset)?)
        } else {
            None
        };
        let multicast_control = if frame_control.multicast_flag() {
            Some(bytes.read(offset)?)
        } else {
            None
        };
        let source_route_subframe = if frame_control.source_flag() {
            Some(bytes.read(offset)?)
        } else {
            None
        };

        let header = Self {
            frame_control,
            destination,
            source,
            radius,
            sequence_number,
            destination_ieee,
            source_ieee,
            multicast_control,
            source_route_subframe,
        };

        Ok((header, *offset))
    }
}

impl_byte! {
    /// 3.3.1.8 Multicast Control Field
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MulticastControl(u8);
}

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
    pub relay_list: &'a [u8],
}

impl<'a> TryRead<'a> for SourceRouteSubframe<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let relay_count = bytes.read(offset)?;
        let relay_index = bytes.read(offset)?;
        let relay_list = bytes.read_with(offset, byte::ctx::Bytes::Len(relay_count as usize))?;

        let subframe = Self {
            relay_count,
            relay_index,
            relay_list,
        };

        Ok((subframe, *offset))
    }
}
