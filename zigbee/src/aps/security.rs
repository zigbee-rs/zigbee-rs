//! APS layer security services (§4.4)

use core::slice;

use byte::BytesExt;
use zigbee_mac::mlme::MacError;
use zigbee_types::ByteArray;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::StorageVec;

use crate::aps::aib;
use crate::aps::aib::DeviceKeyPairDescriptor;
use crate::aps::aib::KeyAttribute;
use crate::aps::aib::LinkKeyType;
use crate::aps::frame::CommandFrame;
use crate::aps::frame::Frame;
use crate::aps::frame::command::Command;
use crate::aps::frame::command::TransportKey;
use crate::aps::frame::frame_control::FrameControl;
use crate::aps::frame::frame_control::FrameType;
use crate::aps::frame::header::Header;
use crate::aps::types::TxOptions;
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
    let b = buf.as_ptr();
    let mut nwk_data = nlme.poll_nwk_data(&mut buf, 5).await?;

    // SAFETY: we can safely take a &mut since it references the buf above
    let aps_buf = unsafe { nwk_data.payload_as_mut() };
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

            // record the TC's IEEE address and install the default
            // link key in the AIB so we can encrypt towards the TC
            let aib = aib::get_ref();
            aib.set_trust_center_address(nwk_key.source_address);
            let mut key_set = aib.device_key_pair_set();
            if !key_set
                .iter()
                .any(|k| k.device_address == nwk_key.source_address)
            {
                let _ = key_set.push(DeviceKeyPairDescriptor {
                    device_address: nwk_key.source_address,
                    key_attributes: KeyAttribute::ProvisionalKey,
                    link_key: ByteArray(crate::security::TRUST_CENTER_LINK_KEY),
                    outgoing_frame_counter: 0,
                    incoming_frame_counter: 0,
                    link_key_type: LinkKeyType::GlobalLinkKey,
                });
                aib.set_device_key_pair_set(key_set);
            }

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

/// Build, encrypt, and send an APS command frame to a specific
/// destination (§4.4).
pub async fn send_aps_command<T: NlmeSap>(
    nlme: &mut T,
    aps_counter: &mut u8,
    destination: ShortAddress,
    dest_ieee: IeeeAddress,
    command: Command,
) -> Result<(), NetworkError> {
    *aps_counter = aps_counter.wrapping_add(1);

    let frame_control = FrameControl::default()
        .set_frame_type(FrameType::Command)
        .set_security_flag(true);

    let header = Header {
        frame_control,
        destination_endpoint: None,
        group_address: None,
        cluster_id: None,
        profile_id: None,
        source_endpoint: None,
        counter: *aps_counter,
        extended_header: None,
    };

    let aps_frame = Frame::ApsCommand(CommandFrame { header, command });

    let mut buf = [0u8; 128];
    let cx = SecurityContext::get();
    let len =
        cx.encrypt_aps_frame_in_place(aps_frame, &mut buf, dest_ieee, TxOptions::default())?;

    nlme.send_data(destination, true, &buf[..len]).await
}

/// Poll the coordinator for an encrypted APS command, decrypt it, and
/// return the parsed command (§4.4).
pub async fn poll_aps_command<T: NlmeSap>(
    nlme: &mut T,
    retries: u8,
) -> Result<Command, NetworkError> {
    let mut buf = [0u8; 128];
    let mut nwk_data = nlme.poll_nwk_data(&mut buf, retries).await?;

    // SAFETY: we can safely take a &mut since it references the buf above
    let aps_buf = unsafe { nwk_data.payload_as_mut() };
    let cx = SecurityContext::get();
    let aps_frame = cx.decrypt_aps_frame_in_place(aps_buf).unwrap();

    let Frame::ApsCommand(CommandFrame { command, .. }) = aps_frame else {
        return Err(NetworkError::ParseError);
    };

    Ok(command)
}
