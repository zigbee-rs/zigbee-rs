use aes::Aes128;
use byte::BytesExt;
use byte::TryRead;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::AeadMutInPlace;
use ccm::consts::U13;
use ccm::consts::U4;
use ccm::Ccm;
use ccm::KeyInit;
use frame::AuxFrameHeader;
use frame::SecurityControl;
use frame::SecurityLevel;
use thiserror::Error;

use crate::common::types::IeeeAddress;
use crate::nwk::frame::header::Header as NwkHeader;

pub mod frame;

// Default ZigbeeAlliance09 key
// centralized security global trust center link key
const TRUST_CENTER_LINK_KEY: &[u8] = &[
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
    pub fn secure_nwk_frame(
        &self,
        nwk_hdr: NwkHeader<'_>,
        frame_buffer: &mut [u8],
    ) -> Result<(), SecurityError> {
        // TODO: get values from NIB
        let aux_hdr = AuxFrameHeader {
            security_control: SecurityControl(0x28),
            frame_counter: 0x1,
            source_address: nwk_hdr.source_ieee,
            key_sequence_number: Some(0x0),
        };
        let mic_length = SecurityLevel::EncMic32.mic_length();

        let nonce_bytes = create_nonce(
            &aux_hdr.source_address.unwrap(),
            aux_hdr.frame_counter,
            &aux_hdr.security_control,
        );
        let nonce = GenericArray::from(nonce_bytes);

        let offset = &mut 0;
        frame_buffer.write_with(offset, nwk_hdr, ())?;
        frame_buffer.write_with(offset, aux_hdr, ())?;
        let (aad, payload) = frame_buffer.split_at_mut(*offset);
        let (payload, tag) = payload.split_at_mut(payload.len() - mic_length);

        // TODO: select the key from NIB
        let key = NETWORK_KEY;
        let mut cipher = Aes128Ccm::new(GenericArray::from_slice(key));

        let t = cipher
            .encrypt_in_place_detached(&nonce, aad, payload)
            .map_err(SecurityError::CcmError)?;
        tag.copy_from_slice(t.as_slice());

        Ok(())
    }

    // section 4.3.1.2
    pub fn unsecure_nwk_frame(&self, frame_buffer: &mut [u8]) -> Result<(), SecurityError> {
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

        let nonce: GenericArray<u8, _> = create_nonce(
            &source_address,
            aux_hdr.frame_counter,
            &aux_hdr.security_control,
        )
        .into();
        let mut cipher = Aes128Ccm::new(key.into());

        cipher
            .decrypt_in_place_detached(&nonce, aad, enc_data, tag)
            .map_err(SecurityError::CcmError)?;

        Ok(())
    }

    // section 4.4.1.2
    pub fn unsecure_aps_frame(&self, frame_buffer: &mut [u8]) -> Result<(), SecurityError> {
        // 5) overwrite the security level with the value from the NIB
        // (default 0x05)
        let sec_level = SecurityLevel::EncMic32;
        let mic_length = sec_level.mic_length();
        byte::check_len(frame_buffer, mic_length)?;

        //let (_, aps_hdr_len) = ApsHeader::try_read(frame_buffer, ())?;
        let aps_hdr_len = 0;
        let (mut aux_hdr, aux_hdr_len) =
            AuxFrameHeader::try_read(&frame_buffer[aps_hdr_len..], ())?;

        if aux_hdr.frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

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

        // TODO: select the key from AIB
        let key = TRUST_CENTER_LINK_KEY;
        let nonce: GenericArray<u8, _> = create_nonce(
            &source_address,
            aux_hdr.frame_counter,
            &aux_hdr.security_control,
        )
        .into();
        let mut cipher = Aes128Ccm::new(key.into());

        cipher
            .decrypt_in_place_detached(&nonce, aad, enc_data, tag)
            .map_err(SecurityError::CcmError)?;

        Ok(())
    }
}

// Figure 4-20
#[allow(clippy::needless_range_loop)]
fn create_nonce(
    source_address: &IeeeAddress,
    frame_counter: u32,
    security_control: &SecurityControl,
) -> [u8; 13] {
    let mut nonce = [0u8; 13];
    for i in 0..8 {
        nonce[i] = (source_address.0 >> (8 * i) & 0xFF) as u8;
    }

    for i in 0..4 {
        nonce[i + 8] = (frame_counter >> (8 * i) & 0xFF) as u8;
    }
    nonce[12] = security_control.0;
    nonce
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_create_nonce() {
        let source_address = IeeeAddress(0xaaaa_bbbb_cccc_dddd);
        let frame_counter = 0x0;
        let security_control = SecurityControl(0x40);
        let nonce = create_nonce(&source_address, frame_counter, &security_control);
        assert_eq!(
            nonce,
            [0xdd, 0xdd, 0xcc, 0xcc, 0xbb, 0xbb, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x40]
        );
    }

    #[test]
    fn decrypt_aps_frame() {
        let frame_buffer = &mut [
            0x21, 0x95, // aps header
            0x30, 0x0, 0x0, 0x0, 0x0, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce,
            0xf4, // aux header
            0xcc, 0x56, 0x50, 0x5e, 0x7, 0x2d, 0xc5, 0xc1, 0xe8, 0x40, 0xf2, 0xd5, 0xce, 0xc, 0xa9,
            0x2d, 0x64, 0x23, 0xcc, 0xc, 0x56, 0xcc, 0xc4, 0xcc, 0xf, 0x18, 0xa2, 0xe4, 0x82, 0x88,
            0x58, 0x4a, 0x90, 0x3e, 0x0, // encrypted data
            0x47, 0x60, 0xf2, 0x5d, // mic
        ];

        let security_context = SecurityContext::new();

        security_context.unsecure_aps_frame(frame_buffer).unwrap();
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

        security_context.unsecure_nwk_frame(frame_buffer).unwrap();
    }
}
