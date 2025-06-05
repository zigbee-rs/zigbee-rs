//! Application Support Sub-Layer Data Entity
//!
//! The APSDE shall provide a data service to the network layer and both ZDO and
//! application objects to enable the transport of application PDUs between two
//! or more devices.
//!
//! it will provide:
//! * Generation of the application level PDU (APDU)
//! * Binding
//! * Group address filtering
//! * Reliable transport
//! * Duplicate rejection
//! * Fragmentation
#![allow(dead_code)]
use crate::aps::types;

pub(crate) mod indication;
pub(crate) mod request;
pub(crate) mod sap;
pub(crate) mod status;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Apsde {
    pub(crate) supports_binding_table: bool,
}

impl Apsde {
    pub(crate) fn new() -> Self {
        Self {
            supports_binding_table: true,
        }
    }
}
