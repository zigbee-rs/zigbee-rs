use byte::{BytesExt, TryRead, TryWrite};

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SrcAddrMode {
    Reserved = 0x00,
    #[default]
    Short,
    Extended = 0x02,
}

impl TryRead<'_, ()> for SrcAddrMode {
    fn try_read(bytes: &'_ [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let id: u8 = bytes.read(offset)?;
        let mode = match id {
            0x00 => Self::Reserved,
            0x02 => Self::Extended,
            _ => Self::Short,
        };

        Ok((mode, *offset))
    }
}

impl TryWrite for SrcAddrMode {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self as u8)?;
        Ok(*offset)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum DstAddrMode {
    #[default]
    None,
    Group = 0x01,
    Network = 0x02,
    Extended = 0x03,
}

impl TryRead<'_, ()> for DstAddrMode {
    fn try_read(bytes: &'_ [u8], _: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let id: u8 = bytes.read(offset)?;
        let mode = match id {
            0x01 => Self::Group,
            0x02 => Self::Network,
            0x03 => Self::Extended,
            _ => Self::None,
        };

        Ok((mode, *offset))
    }
}

impl TryWrite for DstAddrMode {
    fn try_write(self, bytes: &mut [u8], _: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self as u8)?;
        Ok(*offset)
    }
}


