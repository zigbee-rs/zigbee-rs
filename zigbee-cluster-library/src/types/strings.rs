//! String and octet-string data types.
//!
//! See ZCL Section 2.6.2.10 to 2.6.2.13.
//!
//! Four wire types share two shapes: a one-byte length for the short forms and
//! a two-byte length for the long ones, over either UTF-8 text or opaque
//! octets. The maximum length is one below the all-ones length, which is the
//! non-value.

use byte::BytesExt;
use byte::ctx;

use super::codec::ZclKind;
use super::ids::TypeId;

/// Longest `CharacterString` or `OctetString` (2.6.2.10, 2.6.2.12).
pub const SHORT_MAX_LEN: usize = 254;
/// Longest `LongCharacterString` or `LongOctetString` (2.6.2.11, 2.6.2.13).
pub const LONG_MAX_LEN: usize = 65534;

/// A `CharacterString` value: UTF-8, at most [`SHORT_MAX_LEN`] bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortStr<'a>(&'a str);

impl<'a> ShortStr<'a> {
    /// Wraps `s`, rejecting a string too long for the one-byte length.
    pub const fn new(s: &'a str) -> Option<Self> {
        if s.len() > SHORT_MAX_LEN {
            return None;
        }
        Some(Self(s))
    }

    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

/// A `LongCharacterString` value: UTF-8, at most [`LONG_MAX_LEN`] bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongStr<'a>(&'a str);

impl<'a> LongStr<'a> {
    /// Wraps `s`, rejecting a string too long for the two-byte length.
    pub const fn new(s: &'a str) -> Option<Self> {
        if s.len() > LONG_MAX_LEN {
            return None;
        }
        Some(Self(s))
    }

    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

/// Reads a length-prefixed byte run, rejecting the all-ones non-value.
fn read_run<'a, const N: usize>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<&'a [u8]> {
    let len = match N {
        1 => usize::from(bytes.read_with::<u8>(offset, ctx::LE)?),
        _ => usize::from(bytes.read_with::<u16>(offset, ctx::LE)?),
    };
    if len == (1usize << (N * 8)) - 1 {
        return Err(bad_input!("string non-value"));
    }
    let raw = bytes
        .get(*offset..*offset + len)
        .ok_or(byte::Error::Incomplete)?;
    *offset += len;
    Ok(raw)
}

/// Writes a length-prefixed byte run.
fn write_run<const N: usize>(
    value: &[u8],
    bytes: &mut [u8],
    offset: &mut usize,
) -> byte::Result<()> {
    // one below the all-ones length, which is reserved for the non-value
    if value.len() >= (1usize << (N * 8)) - 1 {
        return Err(bad_input!("string too long"));
    }
    #[allow(clippy::cast_possible_truncation)]
    match N {
        // the length was bounds-checked against the prefix width above
        1 => bytes.write_with(offset, value.len() as u8, ctx::LE)?,
        _ => bytes.write_with(offset, value.len() as u16, ctx::LE)?,
    }
    bytes
        .get_mut(*offset..*offset + value.len())
        .ok_or(byte::Error::Incomplete)?
        .copy_from_slice(value);
    *offset += value.len();
    Ok(())
}

fn as_utf8(raw: &[u8]) -> byte::Result<&str> {
    core::str::from_utf8(raw).map_err(|_| bad_input!("invalid utf-8"))
}

/// `CharacterString` (2.6.2.10).
pub struct ShortText;

impl ZclKind for ShortText {
    type Value<'a> = ShortStr<'a>;
    const TYPE_ID: TypeId = TypeId::CharacterString;
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<ShortStr<'a>> {
        Ok(ShortStr(as_utf8(read_run::<1>(bytes, offset)?)?))
    }

    fn write(value: ShortStr<'_>, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
        write_run::<1>(value.0.as_bytes(), bytes, offset)
    }
}

/// `LongCharacterString` (2.6.2.11).
pub struct LongText;

impl ZclKind for LongText {
    type Value<'a> = LongStr<'a>;
    const TYPE_ID: TypeId = TypeId::LongCharacterString;
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<LongStr<'a>> {
        Ok(LongStr(as_utf8(read_run::<2>(bytes, offset)?)?))
    }

    fn write(value: LongStr<'_>, bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
        write_run::<2>(value.0.as_bytes(), bytes, offset)
    }
}

/// `OctetString` (2.6.2.12).
pub struct ShortOctetString;

impl ZclKind for ShortOctetString {
    type Value<'a> = &'a [u8];
    const TYPE_ID: TypeId = TypeId::OctetString;
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<&'a [u8]> {
        read_run::<1>(bytes, offset)
    }

    fn write(value: &[u8], bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
        write_run::<1>(value, bytes, offset)
    }
}

/// `LongOctetString` (2.6.2.13).
pub struct LongOctetString;

impl ZclKind for LongOctetString {
    type Value<'a> = &'a [u8];
    const TYPE_ID: TypeId = TypeId::LongOctetString;
    fn read<'a>(bytes: &'a [u8], offset: &mut usize) -> byte::Result<&'a [u8]> {
        read_run::<2>(bytes, offset)
    }

    fn write(value: &[u8], bytes: &mut [u8], offset: &mut usize) -> byte::Result<()> {
        write_run::<2>(value, bytes, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_round_trips() {
        let mut buf = [0u8; 32];
        let offset = &mut 0;
        ShortText::write(ShortStr::new("zigbee-rs").unwrap(), &mut buf, offset)
            .expect("string encoded");
        assert_eq!(&buf[..*offset], b"\x09zigbee-rs");

        let read = &mut 0;
        let value = ShortText::read(&buf[..*offset], read).expect("string decoded");
        assert_eq!(value.as_str(), "zigbee-rs");
    }

    #[test]
    fn long_text_uses_a_two_byte_length() {
        let mut buf = [0u8; 32];
        let offset = &mut 0;
        LongText::write(LongStr::new("ab").unwrap(), &mut buf, offset).expect("string encoded");
        assert_eq!(&buf[..*offset], b"\x02\x00ab");
    }

    // 2.6.2.10 / 2.6.2.11: the all-ones length is the non-value
    #[test]
    fn the_non_value_length_is_rejected() {
        assert!(ShortText::read(&[0xFF], &mut 0).is_err());
        assert!(LongText::read(&[0xFF, 0xFF], &mut 0).is_err());
    }

    #[test]
    fn invalid_utf8_is_rejected_but_octets_are_not() {
        assert!(ShortText::read(&[0x01, 0xFF], &mut 0).is_err());
        assert_eq!(
            ShortOctetString::read(&[0x01, 0xFF], &mut 0).expect("octets decoded"),
            &[0xFFu8]
        );
    }

    #[test]
    fn a_string_too_long_for_its_length_prefix_is_rejected() {
        let long = [b'a'; 300];
        let text = core::str::from_utf8(&long).unwrap();
        assert!(ShortStr::new(text).is_none());
        assert!(LongStr::new(text).is_some());
    }
}
