//! ZDO Device_annce (§2.4.3.1.11)
//!
//! Broadcast by a device that has joined or re-joined a network to notify
//! other devices of its short address, IEEE address, and capabilities.

use byte::BytesExt;
use zigbee_macros::impl_byte;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;

use crate::aps::apsde;
use crate::nwk::nib::CapabilityInformation;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::NlmeSap;

/// ZDP Device_annce cluster identifier.
const DEVICE_ANNCE_CLUSTER_ID: u16 = 0x0013;
/// ZigBee Device Profile identifier.
const ZDP_PROFILE_ID: u16 = 0x0000;
/// ZDO endpoint.
const ZDO_ENDPOINT: u8 = 0x00;

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

/// Broadcast a ZDO Device_annce (§2.4.3.1.11).
///
/// Serializes the ZDP payload (transaction sequence number +
/// [`DeviceAnnce`]) and hands it to the APS layer for broadcast.
pub async fn broadcast<T: NlmeSap>(
    nlme: &mut T,
    aps_counter: &mut u8,
    annce: DeviceAnnce,
) -> Result<(), NetworkError> {
    let mut zdp_buf = [0u8; 12];
    let offset = &mut 0;
    // ZDP transaction sequence number — reuse the APS counter that will
    // be assigned by the APSDE layer (peek at the next value)
    zdp_buf.write(offset, aps_counter.wrapping_add(1))?;
    zdp_buf.write_with(offset, annce, ())?;

    apsde::broadcast_data(
        nlme,
        aps_counter,
        ZDO_ENDPOINT,
        DEVICE_ANNCE_CLUSTER_ID,
        ZDP_PROFILE_ID,
        ZDO_ENDPOINT,
        &zdp_buf[..*offset],
    )
    .await
}
