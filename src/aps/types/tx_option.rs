use byte::{BytesExt, TryRead, TryWrite};

#[derive(Debug, Clone, Default, PartialEq)]
pub enum TxOptions {
    SecurityEnabled = 0x01,
    UseNetworkKey = 0x02,
    #[default]
    Acknowledged,
    FragmentationPermitted = 0x08,
    IncludeExtendedNonce = 0x10,
}

impl TryRead<'_, ()> for TxOptions {
    fn try_read(bytes: &'_ [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let id: u8 = bytes.read(offset)?;
        let mode = match id {
            0x01 => Self::SecurityEnabled,
            0x02 => Self::UseNetworkKey,
            0x08 => Self::FragmentationPermitted,
            0x10 => Self::IncludeExtendedNonce,
            _ => Self::Acknowledged,
        };

        Ok((mode, *offset))
    }
}

impl TryWrite for TxOptions {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self as u8)?;
        Ok(*offset)
    }
}

