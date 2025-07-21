//! User Descriptor
//!
//! See Section 2.3.2.7
//!
//! The user descriptor contains information that allows the user to identify
//! the device using a user-friendly character string, such as “Bedroom TV” or
//! “Stairs light”. The use of the user descriptor is optional.

use core::cmp::min;

use byte::ctx;
use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;

const USER_DESCRIPTOR_SIZE: usize = 16;

#[derive(Debug)]
pub struct UserDescriptor<'a>(&'a [u8]);

impl<'a> TryRead<'a, ()> for UserDescriptor<'a> {
    fn try_read(bytes: &'a [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let value: &'a [u8] = bytes.read_with(
            offset,
            ctx::Bytes::Len(min(bytes.len(), USER_DESCRIPTOR_SIZE)),
        )?;
        if value.iter().all(|&b| b > 0x1f && b < 0x80) {
            Ok((UserDescriptor(value), *offset))
        } else {
            Err(byte::Error::BadInput {
                err: "InvalidUserDescriptor: Input is not valid ASCII",
            })
        }
    }
}

impl TryWrite for UserDescriptor<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self.0)?;
        Ok(*offset)
    }
}

impl UserDescriptor<'_> {
    fn value(&self) -> &str {
        // Safety: We verify that a user descriptor only contains valid ASCII characters
        // upon creation.
        unsafe { str::from_utf8_unchecked(&self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_user_descriptor_with_valid_name_should_succeed() {
        // given
        let bytes = b"Bedroom TV";

        // when
        let user_descriptor = UserDescriptor::try_read(bytes, ());

        // then
        assert!(user_descriptor.is_ok());
        assert_eq!(user_descriptor.as_ref().unwrap().0.value(), "Bedroom TV");
        assert_eq!(user_descriptor.as_ref().unwrap().1, 10);
    }

    #[test]
    fn creating_user_descriptor_with_invalid_name_should_fail() {
        // given
        // "⭐ light"
        let bytes = &[0xE2, 0xAD, 0x90, b' ', b'l', b'i', b'g', b'h', b't'];

        // when
        let user_descriptor = UserDescriptor::try_read(bytes, ());

        // then
        assert!(user_descriptor.is_err());
        assert_eq!(
            user_descriptor.unwrap_err(),
            byte::Error::BadInput {
                err: "InvalidUserDescriptor: Input is not valid ASCII"
            }
        );
    }

    #[test]
    fn user_descriptor_should_have_max_16_characters() {
        // given
        let bytes = b"85-inch Bedroom TV";

        // when
        let user_descriptor = UserDescriptor::try_read(bytes, ());

        // then
        assert!(user_descriptor.is_ok());
        assert_eq!(
            user_descriptor.as_ref().unwrap().0.value(),
            "85-inch Bedroom "
        );
        assert_eq!(user_descriptor.as_ref().unwrap().1, 16);
    }

    #[test]
    fn writing_user_descriptor_should_succeed() {
        // given
        let mut bytes: [u8; 16] = [0; 16];
        let user_descriptor = UserDescriptor(b"Bedroom TV");

        // when
        let bytes_written = user_descriptor.try_write(&mut bytes, ()).unwrap();

        // then
        assert_eq!(bytes_written, 10);
        assert_eq!(str::from_utf8(&bytes[0..10]).unwrap(), "Bedroom TV");
    }
}
