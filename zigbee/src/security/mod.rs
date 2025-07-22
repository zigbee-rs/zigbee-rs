//! Security Service
//!
//! Security services provided for ZigBee include methods for key establishment, key transport, frame protection, and
//! device management.
use core::convert::TryInto;

use aes::cipher::generic_array::GenericArray as AesGenericArray;
use aes::cipher::BlockEncrypt;
use aes::cipher::KeyInit as AesKeyInit;
use aes::Aes128;
use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::Aead;
use ccm::aead::AeadMutInPlace;
use ccm::consts::U13;
use ccm::consts::U4;
use ccm::Ccm;
use ccm::KeyInit;
use frame::AuxFrameHeader;
use frame::SecurityControl;
use frame::SecurityLevel;
use thiserror::Error;

use crate::aps::apdu::frame::command::Command as ApsCommand;
use crate::aps::apdu::frame::command::TransportKey;
use crate::aps::apdu::frame::frame_control::FrameType as ApsFrameType;
use crate::aps::apdu::frame::header::Header as ApsHeader;
use crate::aps::apdu::frame::Frame as ApsFrame;
use crate::internal::types::IeeeAddress;
use crate::nwk::frame::header::Header as NwkHeader;
use crate::nwk::frame::Frame as NwkFrame;
use crate::security::frame::KeyIdentifier;
use crate::security::primitives::Aes128Mmo;
use crate::security::primitives::HmacAes128Mmo;

pub mod frame;
pub mod primitives;

// Default ZigbeeAlliance09 key
// centralized security global trust center link key
const TRUST_CENTER_LINK_KEY: [u8; 16] = [
    0x5a, 0x69, 0x67, 0x42, 0x65, 0x65, 0x41, 0x6c, 0x6c, 0x69, 0x61, 0x6e, 0x63, 0x65, 0x30, 0x39,
];

const NETWORK_KEY: &[u8] = &[
    0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// AES-128 CCM with MIC32
pub type Aes128Ccm = Ccm<Aes128, U4, U13>;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("invalid key")]
    InvalidKey,
    #[error("invalid data")]
    InvalidData,
    #[error("parse error")]
    ParseError(byte::Error),
    #[error("ccm error")]
    CcmError(ccm::Error),
}

impl From<byte::Error> for SecurityError {
    fn from(value: byte::Error) -> Self {
        Self::ParseError(value)
    }
}

impl From<SecurityError> for byte::Error {
    fn from(value: SecurityError) -> Self {
        match value {
            SecurityError::InvalidKey => Self::BadInput {
                err: "security: invalid key",
            },
            SecurityError::InvalidData => Self::BadInput {
                err: "security: invalid data",
            },
            SecurityError::ParseError(e) => e,
            SecurityError::CcmError(_) => Self::BadInput {
                err: "security: ccm error",
            },
        }
    }
}

pub struct SecurityContext {}

impl SecurityContext {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {}
    }

    pub fn no_security() -> Self {
        Self {}
    }

    // section 4.3.1.1
    pub fn encrypt_nwk_frame_in_place(
        &self,
        nwk_frame: NwkFrame<'_>,
        frame_buffer: &mut [u8],
    ) -> Result<(), SecurityError> {
        // TODO: get values from NIB
        let sec_level = SecurityLevel::EncMic32;
        let mic_len = sec_level.mic_length();
        let frame_counter = 0;
        let key_sequence_number = Some(0);
        let local_addr = IeeeAddress(0xffff_ffff_ffff_ffff);
        let key = NETWORK_KEY;

        let mut security_control = SecurityControl::default();
        security_control.set_security_level(sec_level);
        security_control.set_key_identifier(KeyIdentifier::Network);
        security_control.set_extended_nonce(true);

        let aux_hdr = AuxFrameHeader {
            security_control,
            frame_counter,
            key_sequence_number,
            source_address: Some(local_addr),
        };

        match nwk_frame {
            NwkFrame::Data(data_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                key,
                data_frame.header,
                data_frame.payload,
            ),
            NwkFrame::NwkCommand(command_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                key,
                command_frame.header,
                command_frame.command,
            ),
            NwkFrame::Reserved(header) | NwkFrame::InterPan(header) => {
                // no security required
                frame_buffer
                    .write_with(&mut 0, header, ())
                    .map_err(Into::into)
            }
        }
    }

    // section 4.3.1.2
    pub fn decrypt_nwk_frame_in_place(&self, frame_buffer: &mut [u8]) -> Result<(), SecurityError> {
        // Sec 4.3.1.2: overwrite the security level with the value from the NIB
        // (default 0x05)
        let sec_level = SecurityLevel::EncMic32;

        let mic_length = sec_level.mic_length();
        byte::check_len(frame_buffer, mic_length)?;

        let (_nwk_header, nwk_hdr_len) = NwkHeader::try_read(frame_buffer, ())?;
        let (mut aux_hdr, aux_hdr_len) =
            AuxFrameHeader::try_read(&frame_buffer[nwk_hdr_len..], ())?;

        if aux_hdr.frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

        // TODO: 2) select the key from NIB
        let key = NETWORK_KEY;

        // write back the security level from NIB to aux header
        // the updated values is required as input to ccm
        aux_hdr.security_control.set_security_level(sec_level);
        let mut offset = nwk_hdr_len;
        frame_buffer.write_with(&mut offset, aux_hdr, ())?;

        let (aad, frame) = frame_buffer.split_at_mut(nwk_hdr_len + aux_hdr_len);
        let (enc_data, tag) = frame.split_at_mut(frame.len() - mic_length);
        let tag = GenericArray::from_slice(tag);

        // TODO:verify the source address
        let Some(source_address) = aux_hdr.source_address else {
            return Err(SecurityError::InvalidData);
        };

        let nonce: GenericArray<u8, _> = create_nonce(&aux_hdr)?.into();
        let mut cipher = Aes128Ccm::new(key.into());

        cipher
            .decrypt_in_place_detached(&nonce, aad, enc_data, tag)
            .map_err(SecurityError::CcmError)?;

        Ok(())
    }

    pub fn encrypt_aps_frame_in_place(
        &self,
        aps_frame: ApsFrame<'_>,
        frame_buffer: &mut [u8],
    ) -> Result<(), SecurityError> {
        // Step 1: Obtain security material and key identifier
        let (key, key_id) = match &aps_frame {
            // APSDE-DATA frame
            ApsFrame::Data(data_frame) => {
                // TODO: get link key associated with destination from AIB
                // For now, use a default key
                let key = TRUST_CENTER_LINK_KEY;
                let key_id = KeyIdentifier::Data;
                (key, key_id)
            }
            ApsFrame::ApsCommand(command_frame) => {
                // TODO: get from AIB
                let key = TRUST_CENTER_LINK_KEY;
                let key_id = match command_frame.command {
                    ApsCommand::TransportKey(TransportKey::StandardNetworkKey(_)) => {
                        KeyIdentifier::KeyTransport
                    }
                    ApsCommand::TransportKey(TransportKey::ApplicationLinkKey(_)) => {
                        KeyIdentifier::KeyLoad
                    }
                    _ => KeyIdentifier::Data,
                };
                (key, key_id)
            }
            ApsFrame::Acknowledgement(_) => {
                return Ok(());
            }
        };

        // Step 2: Extract frame counter (and key sequence number if needed)
        // TODO: Get from AIB
        let frame_counter = 0x1;
        if frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

        // Step 3: Obtain security level from NIB
        // TODO: Get from NIB
        let sec_level = SecurityLevel::EncMic32;
        let mic_length = sec_level.mic_length();

        // Set key identifier
        let mut security_control = SecurityControl::default();
        security_control.set_security_level(sec_level);
        security_control.set_key_identifier(key_id);

        // TODO: also set if TxOptions == 0x10
        if matches!(aps_frame, ApsFrame::ApsCommand(_)) {
            security_control.set_extended_nonce(true);
        }

        let source_address = if security_control.extended_nonce() {
            // TODO: get from local device
            Some(IeeeAddress(0x1234_5678_90ab_cdef))
        } else {
            None
        };

        // Step 4: Construct auxiliary header
        let aux_hdr = AuxFrameHeader {
            security_control,
            frame_counter,
            source_address,
            key_sequence_number: None, /* this is should be never set because key_id = 0x01
                                        * (NetworkKey) is invalid */
        };
        let nonce = create_nonce(&aux_hdr)?;

        // Write APS header
        match aps_frame {
            ApsFrame::Data(data_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                &key,
                data_frame.header,
                data_frame.payload,
            ),
            ApsFrame::ApsCommand(command_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                &key,
                command_frame.header,
                command_frame.command,
            ),
            ApsFrame::Acknowledgement(header) => {
                // no encryption for acknowledgements
                frame_buffer
                    .write_with(&mut 0, header, ())
                    .map_err(Into::into)
            }
        }
    }

    fn write_and_encrypt_in_place(
        frame_buffer: &mut [u8],
        aux_hdr: AuxFrameHeader,
        key: &[u8],
        hdr: impl TryWrite,
        payload: impl TryWrite,
    ) -> Result<(), SecurityError> {
        let mic_len = aux_hdr.security_control.security_level().mic_length();
        let nonce = create_nonce(&aux_hdr)?;

        let offset = &mut 0;
        frame_buffer.write_with(offset, hdr, ())?;

        let aux_hdr_offset = *offset;
        frame_buffer.write_with(offset, aux_hdr, ())?;

        let payload_offset = *offset;
        frame_buffer.write_with(offset, payload, ())?;

        let (aad, payload) = frame_buffer.split_at_mut(payload_offset);
        let (payload, tag) = payload.split_at_mut(payload.len() - mic_len);

        let nonce = GenericArray::from(nonce);

        let mut cipher = Aes128Ccm::new(GenericArray::from_slice(key));
        let t = cipher
            .encrypt_in_place_detached(&nonce, aad, payload)
            .map_err(SecurityError::CcmError)?;
        tag.copy_from_slice(t.as_slice());

        // overwrite sec level in aux header with 000
        let mut sec_ctl = SecurityControl(frame_buffer[aux_hdr_offset]);
        sec_ctl.set_security_level(SecurityLevel::None);
        frame_buffer[aux_hdr_offset] = sec_ctl.0;

        Ok(())
    }

    // section 4.4.1.2
    pub fn decrypt_aps_frame_in_place(&self, frame_buffer: &mut [u8]) -> Result<(), SecurityError> {
        // 5) overwrite the security level with the value from the NIB
        // (default 0x05)
        let sec_level = SecurityLevel::EncMic32;
        let mic_length = sec_level.mic_length();
        byte::check_len(frame_buffer, mic_length)?;

        let (_, aps_hdr_len) = ApsHeader::try_read(frame_buffer, ())?;
        let (mut aux_hdr, aux_hdr_len) =
            AuxFrameHeader::try_read(&frame_buffer[aps_hdr_len..], ())?;

        if aux_hdr.frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

        // TODO: select the key from AIB
        let key = match aux_hdr.security_control.key_identifier() {
            KeyIdentifier::Data => TRUST_CENTER_LINK_KEY,
            KeyIdentifier::KeyTransport => {
                // Section 4.5.3: key-transport key uses 1-octet string '0x00'
                HmacAes128Mmo::hmac(&TRUST_CENTER_LINK_KEY, &[0x00])?
            }
            KeyIdentifier::KeyLoad => {
                // Section 4.5.3: key-load key uses 1-octet string '0x02'
                HmacAes128Mmo::hmac(&TRUST_CENTER_LINK_KEY, &[0x02])?
            }
            KeyIdentifier::Network => return Err(SecurityError::InvalidData),
        };

        // write back the security level from NIB to aux header
        // the updated values is required as input to ccm
        aux_hdr.security_control.set_security_level(sec_level);
        let mut offset = aps_hdr_len;
        frame_buffer.write_with(&mut offset, aux_hdr, ())?;

        let (aad, frame) = frame_buffer.split_at_mut(aps_hdr_len + aux_hdr_len);
        let (enc_data, tag) = frame.split_at_mut(frame.len() - mic_length);
        let tag = GenericArray::from_slice(tag);

        // TODO:verify the source address
        let Some(source_address) = aux_hdr.source_address else {
            return Err(SecurityError::InvalidData);
        };

        let nonce: GenericArray<u8, _> = create_nonce(&aux_hdr)?.into();
        let mut cipher = Aes128Ccm::new(&key.into());

        cipher
            .decrypt_in_place_detached(&nonce, aad, enc_data, tag)
            .map_err(SecurityError::CcmError)?;

        Ok(())
    }
}

// Figure 4-20
#[allow(clippy::needless_range_loop)]
fn create_nonce(aux_header: &AuxFrameHeader) -> Result<[u8; 13], SecurityError> {
    let AuxFrameHeader {
        security_control,
        frame_counter,
        source_address: Some(IeeeAddress(source_address)),
        ..
    } = aux_header
    else {
        return Err(SecurityError::InvalidData);
    };

    let mut nonce = [0u8; 13];
    for i in 0..8 {
        nonce[i] = (source_address >> (8 * i) & 0xff) as u8;
    }

    for i in 0..4 {
        nonce[i + 8] = (frame_counter >> (8 * i) & 0xff) as u8;
    }
    nonce[12] = security_control.0;
    Ok(nonce)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_create_nonce() {
        let source_address = Some(IeeeAddress(0xaaaa_bbbb_cccc_dddd));
        let frame_counter = 0x0;
        let security_control = SecurityControl(0x40);
        let aux_hdr = AuxFrameHeader {
            security_control,
            frame_counter,
            source_address,
            key_sequence_number: None,
        };
        let nonce = create_nonce(&aux_hdr).unwrap();
        assert_eq!(
            nonce,
            [0xdd, 0xdd, 0xcc, 0xcc, 0xbb, 0xbb, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x40]
        );
    }

    #[test]
    fn decrypt_aps_frame_data() {
        let frame_buffer = &mut [
            0x21, 0x66, // aps header
            0x20, 0x4, 0x0, 0x0, 0x0, 0xe5, 0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1,
            0xa4, // aux header
            0x1a, 0x31, // enc data
            0xa4, 0xd7, 0xf4, 0xd7, //mic
        ];

        let security_context = SecurityContext::new();

        security_context
            .decrypt_aps_frame_in_place(frame_buffer)
            .unwrap();
    }

    #[test]
    fn decrypt_aps_frame_key_transport() {
        let frame_buffer = &mut [
            0x21, 0x95, // aps hdr
            0x30, 0x0, 0x0, 0x0, 0x0, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce, // aux hdr
            0xf4, 0xcc, 0x56, 0x50, 0x5e, 0x7, 0x2d, 0xc5, 0xc1, 0xe8, 0x40, 0xf2, 0xd5, 0xce, 0xc,
            0xa9, 0x2d, 0x64, 0x23, 0xcc, 0xc, 0x56, 0xcc, 0xc4, 0xcc, 0xf, 0x18, 0xa2, 0xe4, 0x82,
            0x88, 0x58, 0x4a, 0x90, 0x3e, 0x0, // enc data
            0x47, 0x60, 0xf2, 0x5d, // mic
        ];

        let security_context = SecurityContext::new();

        security_context
            .decrypt_aps_frame_in_place(frame_buffer)
            .unwrap();
    }

    #[test]
    fn decrypt_aps_frame_key_load() {
        let frame_buffer = &mut [
            0x21, 0x97, // aps hdr
            0x38, 0x1, 0x0, 0x0, 0x0, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce, // aux hdr
            0xf4, 0xe0, 0x4b, 0x37, 0xdb, 0x35, 0xc7, 0x13, 0x41, 0x71, 0xf0, 0xdf, 0xdb, 0x22,
            0xa5, 0xa1, 0x65, 0xbf, 0xfe, 0x41, 0x5a, 0xb2, 0x5f, 0xd9, 0x85, 0x79, 0x92, 0x5a,
            0xd4, 0xe6, 0x48, 0xfa, 0x6, 0xfb, 0x11, // enc data
            0xb7, 0xc9, 0x4, 0x3e, // mic
        ];

        let security_context = SecurityContext::new();

        security_context
            .decrypt_aps_frame_in_place(frame_buffer)
            .unwrap();
    }

    #[test]
    fn decrypt_nwk_frame() {
        let frame_buffer = &mut [
            0x9, 0x1a, // frame control
            0x0, 0x0, 0xe1, 0xcd, 0x1, 0x93, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce, 0xf4, 0xe5,
            0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1, 0xa4, // nwk header
            0x28, // security control
            0x1, 0x0, 0x0, 0x0, 0xe5, 0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1, 0xa4, 0x0, //aad
            0xa6, 0xac, 0x13, // enc_data
            0xf8, 0x5, 0x7f, 0x53, // mic
        ];
        let security_context = SecurityContext::new();

        security_context
            .decrypt_nwk_frame_in_place(frame_buffer)
            .unwrap();
    }
}
