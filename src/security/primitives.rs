use core::cmp::min;

use aes::cipher::generic_array::GenericArray as AesGenericArray;
use aes::cipher::BlockEncrypt;
use aes::cipher::KeyInit as AesKeyInit;
use aes::Aes128;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::Aead;
use ccm::aead::AeadMutInPlace;
use ccm::consts::U13;
use ccm::consts::U4;
use ccm::Ccm;
use ccm::KeyInit;

use crate::security::SecurityError;

/// AES-MMO (Matyas-Meyer-Oseas) hash function implementation
/// Used for Zigbee key derivation as specified in section 4.5.3
/// Simplified for 16-byte (128-bit) inputs only
pub struct Aes128Mmo {
    state: [u8; Self::BLOCK_SIZE], // 128-bit hash state
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]
impl Aes128Mmo {
    const BLOCK_SIZE: usize = 16;
    const PADDING_THRESHOLD: usize = 2usize.pow(Self::BLOCK_SIZE as u32);

    /// Create a new AES-MMO hash context with zero IV
    pub fn new() -> Self {
        Self {
            state: [0u8; Self::BLOCK_SIZE],
        }
    }

    pub fn initialize(iv: &[u8]) -> Self {
        let mut mmo = Self::new();
        let length = min(Self::BLOCK_SIZE, iv.len());
        mmo.state[..length].copy_from_slice(&iv[..length]);
        mmo
    }

    /// Update the hash with a single 128-bit block
    pub fn update(&mut self, data: &[u8]) -> Result<(), SecurityError> {
        let length = data.len();

        let (padded_data, pad_len) = if length < Self::PADDING_THRESHOLD {
            // l+1+k = 7n (mod 8n)
            let pad_len =
                ((14i32 - (length % Self::BLOCK_SIZE) as i32) % Self::BLOCK_SIZE as i32) as usize;
            let mut padded_data = [0u8; Self::BLOCK_SIZE + 2];
            padded_data[0] = 0x80;
            padded_data[pad_len..pad_len + 2].copy_from_slice(&((length * 8) as u16).to_be_bytes());
            (padded_data, pad_len + 2)
        } else {
            // other padding method not supported
            return Err(SecurityError::InvalidData);
        };

        let data = [data, &padded_data[..pad_len]].concat();
        for block in data.chunks(Self::BLOCK_SIZE) {
            let block_len = block.len();

            // E_i = E(H_{i-1}, X_i)
            let cipher = Aes128::new(&AesGenericArray::from(self.state));
            let mut encrypted_block = *AesGenericArray::from_slice(block);
            cipher.encrypt_block(&mut encrypted_block);

            // H_i = E_i ⊕ X_i (Matyas-Meyer-Oseas)
            for i in 0..Self::BLOCK_SIZE {
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

    pub fn digest_with_iv(iv: &[u8], data: &[u8]) -> Result<[u8; 16], SecurityError> {
        let mut hasher = Self::initialize(iv);
        hasher.update(data)?;
        Ok(hasher.finalize())
    }
}

impl Default for Aes128Mmo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_aes_mmo_test_vector() {
        let message = [0xc0];
        let expected = [
            0xae, 0x3a, 0x10, 0x2a, 0x28, 0xd4, 0x3e, 0xe0, 0xd4, 0xa0, 0x9e, 0x22, 0x78, 0x8b,
            0x20, 0x6c,
        ];

        let result = Aes128Mmo::digest(&message).unwrap();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_hmac_aes_mmo_length_equal_block_size() {
        let message = [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd,
            0xce, 0xcf,
        ];
        let expected = [
            0xa7, 0x97, 0x7e, 0x88, 0xbc, 0x0b, 0x61, 0xe8, 0x21, 0x08, 0x27, 0x10, 0x9a, 0x22,
            0x8f, 0x2d,
        ];

        let result = Aes128Mmo::digest(&message).unwrap();

        assert_eq!(result, expected);
    }
}
