//! APS layer security services (§4.4)

use zigbee_mac::mlme::MacError;
use zigbee_types::StorageVec;

use crate::aps::frame::CommandFrame;
use crate::aps::frame::Frame;
use crate::aps::frame::command::Command;
use crate::aps::frame::command::TransportKey;
use crate::nwk::nib;
use crate::nwk::nib::NetworkSecurityMaterialDescriptor;
use crate::nwk::nlme::NetworkError;
use crate::nwk::nlme::NlmeSap;
use crate::security::SecurityContext;

/// Poll the coordinator for a transport key, decrypt it, and install
/// the network key in the NIB (§4.4.10).
///
/// Retries polling up to 5 times since the Trust Center may not have
/// the transport key queued immediately after association.
pub async fn poll_transport_key<T: NlmeSap>(nlme: &mut T) -> Result<(), NetworkError> {
    let mut buf = [0u8; 128];
    let aps_len = nlme.poll_nwk_data(&mut buf, 5).await?;

    let aps_buf = &mut buf[..aps_len];
    let cx = SecurityContext::get();
    let aps_frame = cx.decrypt_aps_frame_in_place(aps_buf)?;

    let Frame::ApsCommand(CommandFrame {
        command: Command::TransportKey(transport_key),
        ..
    }) = aps_frame
    else {
        return Err(NetworkError::NoTransportKey);
    };

    match transport_key {
        TransportKey::StandardNetworkKey(nwk_key) => {
            log::debug!("[APS-Sec] received network key {:?}", nwk_key.key);

            // install network key in NIB
            let nib = nib::get_ref();
            let mut sec_material = nib.security_material_set();
            sec_material.clear();
            let _ = sec_material.push(NetworkSecurityMaterialDescriptor {
                key_seq_number: nwk_key.sequence_number,
                outgoing_frame_counter: 0,
                incoming_frame_counter_set: StorageVec::new(),
                key: nwk_key.key,
                network_key_type: 0x01,
            });

            nib.set_security_material_set(sec_material);
            nib.set_active_key_seq_number(nwk_key.sequence_number);
        }
        TransportKey::ApplicationLinkKey(_app_key) => (), // TODO
        TransportKey::TrustCenterLinkKey(_tcl_key) => (), // TODO
        TransportKey::Reserved(_) => return Err(NetworkError::NoTransportKey),
    }

    Ok(())
}
