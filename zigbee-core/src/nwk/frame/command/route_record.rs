use zigbee_macros::impl_byte;
use zigbee_types::ShortAddress;
use zigbee_types::TypeArrayCtx;
use zigbee_types::TypeArrayRef;

impl_byte! {
    /// Route Record Command Frame
    #[derive(Debug, Clone)]
    pub struct RouteRecord<'a> {
        pub relay_count: u8,
        #[ctx = TypeArrayCtx::Len(usize::from(relay_count))]
        #[ctx_write = ()]
        pub relay_list: TypeArrayRef<'a, ShortAddress>,
    }
}
