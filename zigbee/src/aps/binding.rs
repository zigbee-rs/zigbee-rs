///! 2.2.8.2.1  Binding Table
use heapless::Vec;
use thiserror::Error;

use super::apsme::basemgt::ApsmeBindRequest;
use super::apsme::basemgt::ApsmeUnbindRequest;
use super::types::Address;

#[derive(Clone, Debug, PartialEq)]
pub enum DesignatedDestination {
    DeviceAddress(u16),
    GroupAddress(u16),
}

pub(crate) struct Binding {
    source: Address,
    endpoint: u8,
    cluster_id: u16,
}

/// 2.2.8.2
pub(crate) struct ApsBindingTable {
    // TODO: limit the size
    entries: Vec<Binding, 265>,
}

pub(crate) struct ApsGroupTable {}

impl ApsBindingTable {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    pub(crate) fn is_full(&self) -> bool {
        self.entries.len() >= self.entries.capacity()
    }

    pub(crate) fn create_binding_link(
        &mut self,
        request: &ApsmeBindRequest,
    ) -> Result<(), BindingError> {
        if self.is_full() {
            return Err(BindingError::TableFull);
        }

        self.entries
            .push(Binding {
                source: request.src_address.clone(),
                endpoint: request.dst_endpoint,
                cluster_id: request.cluster_id,
            })
            .map_err(|_| BindingError::TableFull)
    }

    pub(crate) fn remove_binding_link(
        &self,
        _request: &ApsmeUnbindRequest,
    ) -> Result<(), BindingError> {
        Err(BindingError::InvalidBinding)
    }
}

#[derive(Error, Debug)]
#[error("SourceError")]
pub(crate) enum BindingError {
    IllegalRequest,
    InvalidBinding,
    TableFull,
}
