use zigbee_macros::impl_byte;
use zigbee_types::TypeArrayCtx;
use zigbee_types::TypeArrayRef;

impl_byte! {
    /// Network Report Command Frame
    #[derive(Debug, Clone)]
    pub struct NetworkReport<'a> {
        pub report_type: u8,
        pub device_count: u8,
        #[ctx = TypeArrayCtx::Len(usize::from(device_count))]
        #[ctx_write = ()]
        pub device_list: TypeArrayRef<'a, DeviceListEntry>,
    }
}

impl_byte! {
    /// Device List Entry
    #[derive(Debug, Clone)]
    #[repr(packed, Rust)]
    pub struct DeviceListEntry {
        pub device_address: u16,
        pub device_type: u8,
    }
}
