#![allow(dead_code)]

use super::error::ApsError;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SrcAddrMode {
    Reserved = 0x00,
    #[default]
    Short,
    Extended = 0x02,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum DstAddrMode {
    #[default]
    None,
    Group = 0x01,
    Network = 0x02,
    Extended = 0x03,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Address {
    #[default]
    None,
    Group(u16),
    Network(u16),
    Extended(u64),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum TxOptions {
    SecurityEnabled = 0x01,
    UseNetworkKey = 0x02,
    #[default]
    Acknowledged,
    FragmentationPermitted = 0x08,
    IncludeExtendedNonce = 0x10,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SrcEndpoint {
    pub(crate) value: u8,
}

impl SrcEndpoint {
    pub fn new(value: u8) -> Result<Self, ApsError> {
        if value <= 254 {
            Ok(SrcEndpoint { value })
        } else {
            Err(ApsError::InvalidValue)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_value_should_succeed() {
        let src_endpoint = SrcEndpoint::new(254);

        assert!(src_endpoint.is_ok());
    }

    #[test]
    fn oversized_value_should_fail() {
        let src_endpoint = SrcEndpoint::new(255);

        assert!(src_endpoint.is_err());
    }
}
