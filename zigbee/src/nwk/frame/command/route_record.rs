use crate::internal::macros::impl_byte;
use crate::internal::types::ShortAddress;
use crate::internal::types::TypeArrayCtx;
use crate::internal::types::TypeArrayRef;

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
