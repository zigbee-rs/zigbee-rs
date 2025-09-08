#![warn(dead_code)]
use core::cmp::min;
use core::ops::Deref;

use aes::cipher::generic_array::GenericArray as AesGenericArray;
use aes::cipher::BlockEncrypt;
use aes::cipher::KeyInit as AesKeyInit;
use aes::Aes128;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::AeadMutInPlace;
use ccm::consts::U13;
use ccm::consts::U4;
use ccm::Ccm;
use ccm::KeyInit;
use itertools::Itertools;

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
        self.update_impl(data.iter().copied())
    }

    /// Update the hash with a single 128-bit block
    fn update_impl(&mut self, data: impl Iterator<Item = u8>) -> Result<(), SecurityError> {
        let data = data.into_iter();
        let (_, Some(length)) = data.size_hint() else {
            unreachable!("invalid unbounded data");
        };

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

        let pad = &padded_data[..pad_len];
        let mut iter = data.chain(pad.iter().copied());
        let len = length + pad_len;
        let mut i = 0;
        while i < len {
            let mut block = [0u8; Self::BLOCK_SIZE];
            for b_i in 0..Self::BLOCK_SIZE {
                let b = iter.next();
                if let Some(b) = b {
                    i += 1;
                    block[b_i] = b;
                }
            }

            // E_i = E(H_{i-1}, X_i)
            let cipher = Aes128::new(&AesGenericArray::from(self.state));
            let mut encrypted_block = AesGenericArray::from(block);
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

    pub fn digest(data: &[u8]) -> Result<[u8; Self::BLOCK_SIZE], SecurityError> {
        Self::digest_impl(data.iter().copied())
    }

    pub fn digest_impl(
        data: impl Iterator<Item = u8>,
    ) -> Result<[u8; Self::BLOCK_SIZE], SecurityError> {
        let mut hasher = Self::new();
        hasher.update_impl(data)?;
        Ok(hasher.finalize())
    }

    pub fn digest_with_iv(iv: &[u8], data: &[u8]) -> Result<[u8; Self::BLOCK_SIZE], SecurityError> {
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

/// HMAC implementation using AES-MMO hash function
/// Follows RFC 2104 HMAC specification
pub struct HmacAes128Mmo;

impl HmacAes128Mmo {
    const IPAD: u8 = 0x36; // Inner padding byte
    const OPAD: u8 = 0x5c; // Outer padding byte

    /// Convenience method to compute HMAC in one step
    /// $\text{HMAC}(K, M) = H((K \oplus \text{opad}) || H((K \oplus
    /// \text{ipad}) || M))$
    pub fn hmac(key: &[u8], data: &[u8]) -> Result<[u8; Aes128Mmo::BLOCK_SIZE], SecurityError> {
        if key.len() == Aes128Mmo::BLOCK_SIZE {
            return Self::hmac_impl(key, data);
        }

        let key = Aes128Mmo::digest(key)?;
        Self::hmac_impl(&key, data)
    }

    fn hmac_impl(key: &[u8], data: &[u8]) -> Result<[u8; Aes128Mmo::BLOCK_SIZE], SecurityError> {
        let mut ipad = [Self::IPAD; Aes128Mmo::BLOCK_SIZE];
        let mut opad = [Self::OPAD; Aes128Mmo::BLOCK_SIZE];

        for i in 0..Aes128Mmo::BLOCK_SIZE {
            ipad[i] ^= key[i];
            opad[i] ^= key[i];
        }

        let inner = ipad.iter().chain(data.iter());
        let inner_hash = Aes128Mmo::digest_impl(inner.copied())?;

        let outer = opad.iter().chain(inner_hash.iter());
        Aes128Mmo::digest_impl(outer.copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_mmo_test_vector() {
        let message = [0xc0];
        let want = [
            0xae, 0x3a, 0x10, 0x2a, 0x28, 0xd4, 0x3e, 0xe0, 0xd4, 0xa0, 0x9e, 0x22, 0x78, 0x8b,
            0x20, 0x6c,
        ];

        let result = Aes128Mmo::digest(&message).unwrap();

        assert_eq!(result, want);
    }

    #[test]
    fn aes_mmo_length_equal_block_size() {
        let message = [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd,
            0xce, 0xcf,
        ];
        let want = [
            0xa7, 0x97, 0x7e, 0x88, 0xbc, 0x0b, 0x61, 0xe8, 0x21, 0x08, 0x27, 0x10, 0x9a, 0x22,
            0x8f, 0x2d,
        ];

        let result = Aes128Mmo::digest(&message).unwrap();

        assert_eq!(result, want);
    }

    #[test]
    fn hmac_aes_mmo_1() {
        let key = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
            0x4e, 0x4f,
        ];
        let want = [
            0x45, 0x12, 0x80, 0x7b, 0xf9, 0x4c, 0xb3, 0x40, 0x0f, 0x0e, 0x2c, 0x25, 0xfb, 0x76,
            0xe9, 0x99,
        ];
        let message = [0xc0];

        let result = HmacAes128Mmo::hmac(&key, &message).unwrap();

        assert_eq!(result, want);
    }

    #[test]
    fn hmac_aes_mmo_2() {
        let key = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
            0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b,
            0x5c, 0x5d, 0x5e, 0x5f,
        ];
        let want = [
            0xa3, 0xb0, 0x07, 0x99, 0x84, 0xbf, 0x15, 0x57, 0xf7, 0x4a, 0x0d, 0x63, 0x87, 0xe0,
            0xa1, 0x1a,
        ];
        let message = [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd,
            0xce, 0xcf,
        ];

        let result = HmacAes128Mmo::hmac(&key, &message).unwrap();

        assert_eq!(result, want);
    }
}
