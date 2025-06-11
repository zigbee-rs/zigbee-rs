#![allow(dead_code)]

use byte::{BytesExt, TryRead, TryWrite};

use super::error::ApsError;
use crate::impl_byte;

pub mod addr_mode;
pub mod tx_option;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Address {
    #[default]
    None,
    Group(u16),
    Network(u16),
    Extended(u64),
}


impl_byte! {
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct SrcEndpoint {
        pub(crate) value: u8,
    }
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
