//! ZDP Device_annce frame payload (§2.4.3.1.11)
//!
//! Broadcast by a device that has joined or re-joined a network to notify
//! other devices of its short address, IEEE address, and capabilities.

use zigbee_macros::impl_byte;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

use crate::nwk::nib::CapabilityInformation;

/// ZDP Device_annce cluster identifier.
pub const CLUSTER_ID: u16 = 0x0013;

impl_byte! {
    /// ZDP Device_annce payload (§2.4.3.1.11).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DeviceAnnce {
        /// NWK address for the local device.
        pub nwk_addr: ShortAddress,
        /// IEEE address for the local device.
        pub ieee_addr: IeeeAddress,
        /// Capability of the local device.
        pub capability: CapabilityInformation,
    }
}
