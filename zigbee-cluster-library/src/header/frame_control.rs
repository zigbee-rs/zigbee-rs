//! Frame Control
use core::fmt;
use core::fmt::Debug;
use core::mem;

use zigbee_macros::impl_byte;

impl_byte! {
    /// See Section 2.4.1.1
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct FrameControl(pub u8);
}

/// Frame Type
///
/// See Section 2.4.1.1.1.
#[allow(missing_docs)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    GlobalCommand = 0b00,
    ClusterCommand = 0b01,
    Reserved = 0b10,
}

impl FrameControl {
    /// See Section 2.4.1.1.1
    ///
    /// Returns `true` if command is global
    pub fn frame_type(self) -> FrameType {
        // SAFETY: any 2 bit permutation is a valid FrameType
        unsafe { mem::transmute((self.0 & mask::FRAME_TYPE) >> offset::FRAME_TYPE) }
    }

    /// The Manufacturer Specific field specifies whether this command refers to
    /// a manufacturer specific extension.
    ///
    /// If this value is set to 1, the manufacturer code field SHALL be present
    /// in the ``ZCLframe``.
    ///
    /// See Section 2.4.1.1.2
    pub fn is_manufacturer_specific(self) -> bool {
        ((self.0 & mask::MANUFACTURER_SPECIFIC) >> offset::MANUFACTURER_SPECIFIC) != 0
    }

    /// The direction specifies the client/server direction for this command.
    /// If set to 1, the command is being sent from the server side of a
    /// cluster to the client side of a cluster.
    ///
    /// See Section 2.4.1.1.3
    pub fn direction(self) -> bool {
        (self.0 & mask::DIRECTION) != 0
    }

    /// See Section 2.4.1.1.4
    pub fn disable_default_response(self) -> bool {
        (self.0 & mask::DEFAULT_RESPONSE) != 0
    }
}

mod mask {
    pub(super) const FRAME_TYPE: u8 = 0b0000_0011; // 2 bits
    pub(super) const MANUFACTURER_SPECIFIC: u8 = 0b0000_0100; // 1 bit
    pub(super) const DIRECTION: u8 = 0b0000_1000; // 1 bit
    pub(super) const DEFAULT_RESPONSE: u8 = 0b0001_0000; // 1 bit
}
mod offset {
    pub(super) const FRAME_TYPE: u8 = 0;
    pub(super) const MANUFACTURER_SPECIFIC: u8 = 2;
}

impl Debug for FrameControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameControl")
            .field("frame_type", &self.frame_type())
            .field("manufacturer_specific", &self.is_manufacturer_specific())
            .field("direction", &self.direction())
            .field("disable_default_response", &self.disable_default_response())
            .finish()
    }
}

#[cfg(test)]
mod tests {

    use byte::TryRead;

    use super::*;

    #[test]
    fn unpack_frame_control() {
        // given
        let input = [0x18];

        // when
        let (frame_control, _) =
            FrameControl::try_read(&input, ()).expect("Could not read FrameControl in test.");

        // then
        assert_eq!(frame_control.frame_type(), FrameType::GlobalCommand);
        assert!(!frame_control.is_manufacturer_specific());
        assert!(frame_control.direction());
        assert!(frame_control.disable_default_response());
    }

    #[test]
    fn frame_control_with_local_command() {
        // given
        let input = [0x19];

        // when
        let (frame_control, _) =
            FrameControl::try_read(&input, ()).expect("Could not read FrameControl in test.");

        // then
        assert_eq!(frame_control.frame_type(), FrameType::ClusterCommand);
        assert!(!frame_control.is_manufacturer_specific());
        assert!(frame_control.direction());
        assert!(frame_control.disable_default_response());
    }

    #[test]
    fn frame_control_with_manufacturer_specific_flag() {
        // given
        let input = [0x1d];

        // when
        let (frame_control, _) =
            FrameControl::try_read(&input, ()).expect("Could not read FrameControl in test.");

        // then
        assert_eq!(frame_control.frame_type(), FrameType::ClusterCommand);
        assert!(frame_control.is_manufacturer_specific());
        assert!(frame_control.direction());
        assert!(frame_control.disable_default_response());
    }
    #[test]
    fn frame_control_with_direction_server_to_client() {
        // given
        let input = [0x0d];

        // when
        let (frame_control, _) =
            FrameControl::try_read(&input, ()).expect("Could not read FrameControl in test.");

        // then
        assert_eq!(frame_control.frame_type(), FrameType::ClusterCommand);
        assert!(frame_control.is_manufacturer_specific());
        assert!(frame_control.direction());
        assert!(!frame_control.disable_default_response());
    }
}
