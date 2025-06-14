use core::convert::TryInto;

use aes::cipher::generic_array::GenericArray as AesGenericArray;
use aes::cipher::BlockEncrypt;
use aes::cipher::KeyInit as AesKeyInit;
use aes::Aes128;
use byte::BytesExt;
use byte::TryRead;
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

use crate::aps::apdu::header::Header as ApsHeader;
use crate::common::types::IeeeAddress;
use crate::nwk::frame::header::Header as NwkHeader;
use crate::security::frame::KeyIdentifier;

pub mod frame;

const BLOCK_SIZE: usize = 16;

/// AES-MMO (Matyas-Meyer-Oseas) hash function implementation
/// Used for Zigbee key derivation as specified in section 4.5.3
/// Simplified for 16-byte (128-bit) inputs only
pub struct Aes128Mmo {
    state: [u8; BLOCK_SIZE], // 128-bit hash state
}

impl Aes128Mmo {
    /// Create a new AES-MMO hash context with zero IV
    pub fn new() -> Self {
        Self {
            state: [0u8; 16], // Initialize with zero IV
        }
    }

    /// Update the hash with a single 128-bit block
    pub fn update(&mut self, data: &[u8]) -> Result<(), SecurityError> {
        let length = data.len();
        // note: should be able to handle inputs of length <= 2^(2n)
        // but cases of 2^n <= length <= 2^(2n) are not relevant
        // for our use case anyways
        #[allow(clippy::cast_possible_truncation)]
        if length > 2usize.pow(BLOCK_SIZE as u32) {
            return Err(SecurityError::InvalidData);
        }

        for block in data.chunks(BLOCK_SIZE) {
            // padding
            let block_len = block.len();
            let mut padded_block = [0u8; BLOCK_SIZE];
            padded_block[..block_len].copy_from_slice(block);
            if block_len < BLOCK_SIZE {
                padded_block[block_len] = 0b1000_0000;
            }

            // E_i = E(H_{i-1}, X_i)
            let cipher = Aes128::new(&AesGenericArray::from(self.state));
            let mut encrypted_block = *AesGenericArray::from_slice(&padded_block);
            cipher.encrypt_block(&mut encrypted_block);

            // H_i = E_i ⊕ X_i  (Matyas-Meyer-Oseas)
            for i in 0..BLOCK_SIZE {
                self.state[i] = encrypted_block[i] ^ block[i];
            }
        }

        Ok(())
    }

    pub fn finalize(self) -> [u8; 16] {
        self.state
    }

    pub fn digest(data: &[u8]) -> Result<[u8; 16], SecurityError> {
        let mut hasher = Self::new();
        hasher.update(data)?;
        Ok(hasher.finalize())
    }
}

impl Default for Aes128Mmo {
    fn default() -> Self {
        Self::new()
    }
}

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
                Aes128Mmo::digest(&TRUST_CENTER_LINK_KEY)?
            }
            KeyIdentifier::KeyLoad => {
                // Section 4.5.3: key-load key uses 1-octet string '0x02'
                Aes128Mmo::digest(&TRUST_CENTER_LINK_KEY)?
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

        let nonce: GenericArray<u8, _> = create_nonce(
            &source_address,
            aux_hdr.frame_counter,
            &aux_hdr.security_control,
        )
        .into();
        let mut cipher = Aes128Ccm::new(&key.into());

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
        nonce[i] = (source_address.0 >> (8 * i) & 0xff) as u8;
    }

    for i in 0..4 {
        nonce[i + 8] = (frame_counter >> (8 * i) & 0xff) as u8;
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
    fn decrypt_aps_frame_data() {
        let frame_buffer = &mut [
            0x21, 0x66, // aps header
            0x20, 0x4, 0x0, 0x0, 0x0, 0xe5, 0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1,
            0xa4, // aux header
            0x1a, 0x31, // enc data
            0xa4, 0xd7, 0xf4, 0xd7, //mic
        ];

        let security_context = SecurityContext::new();

        security_context.unsecure_aps_frame(frame_buffer).unwrap();
    }

    #[test]
    fn decrypt_aps_frame_key_load() {
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

    #[test]
    fn test_hmac_aes_mmo_test_vector() {
        // Test vector from Zigbee spec C.6.1
        let key = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D,
            0x4E, 0x4F,
        ];
        let message = [0xC0];
        let expected = [
            0x45, 0x12, 0x80, 0x7B, 0xF9, 0x4C, 0xB3, 0x40, 0x0F, 0x0E, 0x2C, 0x25, 0xFB, 0x76,
            0xE9, 0x99,
        ];

        let result = Aes128Mmo::digest(&message).unwrap();
        assert_eq!(result, expected, "HMAC-AES-MMO test vector failed");
    }
}
