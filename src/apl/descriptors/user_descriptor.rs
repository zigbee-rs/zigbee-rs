//! 2.3.2.7 User Descriptor
//! The user descriptor contains information that allows the user to identify the device using a user-friendly character string,
//! such as “Bedroom TV” or “Stairs light”.
//! The use of the user descriptor is optional.
//!

use core::str;

use crate::apl::descriptors::error::Error;
use heapless::Vec;

const USER_DESCRIPTOR_SIZE: usize = 16;

#[derive(Debug)]
pub struct UserDescriptor(Vec<u8, USER_DESCRIPTOR_SIZE>);

impl UserDescriptor {
    fn new(value: Vec<u8, USER_DESCRIPTOR_SIZE>) -> Result<Self, Error> {
        if value.iter().all(|&b| b > 0x1f && b < 0x80) {
            Ok(UserDescriptor(value))
        } else {
            Err(Error::InvalidUserDescriptor)
        }
    }

    fn value(&self) -> &str {
        // Safety: We verify that a user descriptor only contains valid ASCII characters upon creation.
        unsafe { str::from_utf8_unchecked(&self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_user_descriptor_with_valid_name_should_succeed() {
        // given
        let mut bytes: Vec<u8, USER_DESCRIPTOR_SIZE> = Vec::new();
        bytes.extend_from_slice(b"Bedroom TV").unwrap();

        // when
        let user_descriptor = UserDescriptor::new(bytes);

        // then
        assert!(user_descriptor.is_ok());
        assert_eq!(user_descriptor.unwrap().value(), "Bedroom TV");
    }

    #[test]
    fn creating_user_descriptor_with_invalid_name_should_fail() {
        // given
        let mut bytes: Vec<u8, USER_DESCRIPTOR_SIZE> = Vec::new();
        // "⭐ light"
        bytes
            .extend_from_slice(&[0xE2, 0xAD, 0x90, b' ', b'l', b'i', b'g', b'h', b't'])
            .unwrap();

        // when
        let user_descriptor = UserDescriptor::new(bytes);

        // then
        assert!(user_descriptor.is_err());
        assert_eq!(user_descriptor.unwrap_err(), Error::InvalidUserDescriptor);
    }
}
