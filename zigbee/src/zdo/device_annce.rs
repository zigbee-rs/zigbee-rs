//! ZDO Device_annce broadcast (§2.4.3.1.11, §2.5.3)
//!
//! ZDO sends Device_annce via the APSDE-SAP on endpoint 0.

use byte::BytesExt;

pub use crate::zdp::device_annce::DeviceAnnce;
use crate::aps::apsme::Apsme;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::NlmeSap;
use crate::zdp::device_annce::CLUSTER_ID;

/// ZigBee Device Profile identifier.
const ZDP_PROFILE_ID: u16 = 0x0000;
/// ZDO endpoint.
const ZDO_ENDPOINT: u8 = 0x00;

/// Broadcast a ZDO Device_annce (§2.4.3.1.11).
///
/// Serializes the ZDP payload (transaction sequence number +
/// [`DeviceAnnce`]) and hands it to the APS layer for broadcast.
pub async fn broadcast<T: NlmeSap>(
    nlme: &mut T,
    apsme: &mut Apsme,
    zdp_seq: u8,
    annce: DeviceAnnce,
) -> Result<(), NetworkError> {
    let mut zdp_buf = [0u8; 12];
    let offset = &mut 0;
    zdp_buf.write(offset, zdp_seq)?;
    zdp_buf.write_with(offset, annce, ())?;

    // §2.4.3.1.11: broadcast to all RxOnWhenIdle devices
    let rx_on_when_idle = zigbee_types::ShortAddress(0xFFFD);
    apsme
        .broadcast_data(
            nlme,
            rx_on_when_idle,
            ZDO_ENDPOINT,
            CLUSTER_ID,
            ZDP_PROFILE_ID,
            ZDO_ENDPOINT,
            &zdp_buf[..*offset],
        )
        .await
}
