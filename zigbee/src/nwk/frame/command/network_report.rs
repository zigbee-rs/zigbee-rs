pub struct NetworkReport<'a> {
    pub report_type: u8,
    pub device_count: u8,
    pub device_list: &'a [DeviceListEntry],
}

pub struct DeviceListEntry {
    pub device_address: u16,
    pub device_type: u8,
}
