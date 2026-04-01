//! APS layer security services (§4.4)

use zigbee_mac::mlme::MacError;
use zigbee_types::StorageVec;

use crate::nwk::nib;
use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::NlmeSap;
use crate::security::SecurityContext;
use crate::security::TransportKeyIndication;

/// Poll the coordinator for a transport key, decrypt it, and install
/// the network key in the NIB (§4.4.10).
///
/// Retries polling up to 5 times since the Trust Center may not have
/// the transport key queued immediately after association.
pub async fn await_transport_key<T: NlmeSap>(nlme: &mut T) -> Result<(), NetworkError> {
    let mut buf = [0u8; 128];
    let mut aps_len = 0usize;
    let mut received = false;
    for _ in 0..5u8 {
        match nlme.poll_nwk_data(&mut buf).await {
            Ok(n) => {
                aps_len = n;
                received = true;
                break;
            }
            Err(NetworkError::MacError(MacError::NoData)) => continue,
            Err(e) => return Err(e),
        }
    }
    if !received {
        return Err(NetworkError::NoTransportKey);
    }

    let aps_buf = &mut buf[..aps_len];
    let cx = SecurityContext::get();
    let aps_frame = cx.decrypt_aps_frame_in_place(aps_buf)?;
    let indication = TransportKeyIndication::try_from(aps_frame)?;

    // install network key in NIB
    let nib = nib::get_ref();
    let mut sec_material = nib.security_material_set();
    sec_material.clear();
    let _ = sec_material.push(NetworkSecurityMaterialDescriptor {
        key_seq_number: indication.key_seq_number,
        outgoing_frame_counter: 0,
        incoming_frame_counter_set: StorageVec::new(),
        key: indication.key,
        network_key_type: 0x01,
    });
    nib.set_security_material_set(sec_material);
    nib.set_active_key_seq_number(indication.key_seq_number);

    Ok(())
}
