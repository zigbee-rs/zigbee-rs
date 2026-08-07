//! Binding table (2.2.8.2.1).
//!
//! A binding maps a local source endpoint and cluster onto a remote
//! destination, so an application can transmit without naming an address
//! (`DstAddrMode::None`, 2.2.8.4.1).
use thiserror::Error;
use zigbee_macros::impl_byte;
use zigbee_types::IeeeAddress;
use zigbee_types::StorageVec;

impl_byte! {
    #[tag(u8)]
    /// Destination addressing mode of a binding (2.4.3.2.2).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BindingAddrMode {
        /// 16-bit group address, destination endpoint omitted
        Group = 0x01,
        /// 64-bit extended address plus destination endpoint
        Device = 0x03,
        #[fallback = true]
        Reserved(u8),
    }
}

impl BindingAddrMode {
    /// Whether the binding names a group.
    pub fn is_group(self) -> bool {
        self == Self::Group
    }

    /// Whether the binding names a single device endpoint.
    pub fn is_device(self) -> bool {
        self == Self::Device
    }
}

impl Default for BindingAddrMode {
    fn default() -> Self {
        Self::Device
    }
}

impl_byte! {
    /// A binding table entry (2.2.8.2.1).
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct Binding {
        /// local endpoint the binding applies to
        pub src_endpoint: u8,
        /// cluster the binding applies to
        pub cluster_id: u16,
        pub dst_addr_mode: BindingAddrMode,
        #[parse_if = dst_addr_mode.is_group()]
        pub dst_group: Option<u16>,
        #[parse_if = dst_addr_mode.is_device()]
        pub dst_ieee: Option<IeeeAddress>,
        #[parse_if = dst_addr_mode.is_device()]
        pub dst_endpoint: Option<u8>,
    }
}

impl Binding {
    /// A binding to a single remote endpoint.
    pub fn device(
        src_endpoint: u8,
        cluster_id: u16,
        dst_ieee: IeeeAddress,
        dst_endpoint: u8,
    ) -> Self {
        Self {
            src_endpoint,
            cluster_id,
            dst_addr_mode: BindingAddrMode::Device,
            dst_group: None,
            dst_ieee: Some(dst_ieee),
            dst_endpoint: Some(dst_endpoint),
        }
    }

    /// A binding to a group address.
    pub fn group(src_endpoint: u8, cluster_id: u16, dst_group: u16) -> Self {
        Self {
            src_endpoint,
            cluster_id,
            dst_addr_mode: BindingAddrMode::Group,
            dst_group: Some(dst_group),
            dst_ieee: None,
            dst_endpoint: None,
        }
    }

    /// Whether the entry routes traffic of `src_endpoint`/`cluster_id`.
    pub fn matches(&self, src_endpoint: u8, cluster_id: u16) -> bool {
        self.src_endpoint == src_endpoint && self.cluster_id == cluster_id
    }
}

/// The binding table as held in the AIB.
pub type BindingTable<const N: usize> = StorageVec<Binding, N>;

/// Create a binding link (2.2.4.3.1), rejecting an exact duplicate.
pub(crate) fn create_binding_link<const N: usize>(
    table: &mut BindingTable<N>,
    binding: Binding,
) -> Result<(), BindingError> {
    if table.contains(&binding) {
        return Ok(());
    }
    table.push(binding).map_err(|_| BindingError::TableFull)
}

/// Remove a binding link (2.2.4.3.3).
pub(crate) fn remove_binding_link<const N: usize>(
    table: &mut BindingTable<N>,
    binding: &Binding,
) -> Result<(), BindingError> {
    let Some(index) = table.iter().position(|entry| entry == binding) else {
        return Err(BindingError::InvalidBinding);
    };
    table.remove(index);
    Ok(())
}

#[derive(Error, Debug)]
#[error("SourceError")]
pub(crate) enum BindingError {
    IllegalRequest,
    InvalidBinding,
    TableFull,
}
