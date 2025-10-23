use crate::impl_byte;
use crate::internal::types::ByteArray;
use crate::internal::types::IeeeAddress;

impl_byte! {
    #[tag(u8)]
    /// Request Key Command Frame
    /// Table 4-19
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RequestKey {
        #[tag_value = 0x02]
        ApplicationLinkKey(IeeeAddress),
        TrustCenterLinkKey = 0x04,
        #[fallback = true]
        Reserved(u8),
    }
}
