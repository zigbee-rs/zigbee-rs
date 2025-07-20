use heapless::LinearMap;
use heapless::Vec;

/// 2.2.7.2 - AIB (APS Information Base Attributes)
#[derive(Default)]
pub struct ApsInformationBase {
    attributes: LinearMap<u8, AIBAttribute, 265>,
}

impl ApsInformationBase {
    pub fn new() -> Self {
        Self {
            attributes: LinearMap::new(),
        }
    }
    pub fn get_attribute(&self, id: u8) -> Option<&AIBAttribute> {
        self.attributes.get(&id)
    }

    pub fn write_attribute_value(
        &mut self,
        _id: u8,
        _value: AIBAttribute,
    ) -> Result<(), &'static str> {
        // self.attributes.insert(id, value).map_err(|_| "could not insert");

        Ok(())
    }
}

pub type AIBAttributeValue = [u8; 8];

#[derive(Debug, Clone, Default, PartialEq)]
pub enum AIBAttribute {
    #[default]
    ApsBindingTable,
    ApsDesignatedCoordinator(bool),
    ApsChannelMaskList(Vec<u8, 265>),
    ApsUseExtendedPanId(u64),
    ApsGroupTable(u8),
    ApsNonmemberRadius(u8),
    ApsUseInsecureJoin(bool),
    ApsInterframeDelay(u8),
    ApsLastChannelEnergy(u8),
    ApsLastChannelFailureRate(u8),
    ApsChannelTimer(u8),
    ApsMaxWindowSize(u8),
    ApsParentAnnounceTimer(u8),
}

impl AIBAttribute {
    pub fn id(&self) -> u8 {
        match self {
            // 0xc0 removed in 2007
            AIBAttribute::ApsBindingTable => 0xc1,
            AIBAttribute::ApsDesignatedCoordinator(_) => 0xc2,
            AIBAttribute::ApsChannelMaskList(_) => 0xc3,
            AIBAttribute::ApsUseExtendedPanId(_) => 0xc4,
            AIBAttribute::ApsGroupTable(_) => 0xc5,
            AIBAttribute::ApsNonmemberRadius(_) => 0xc6,
            // 0xc7 removed (ApsIpIdPermissionsConfig)
            AIBAttribute::ApsUseInsecureJoin(_) => 0xc8,
            AIBAttribute::ApsInterframeDelay(_) => 0xc9,
            AIBAttribute::ApsLastChannelEnergy(_) => 0xca,
            AIBAttribute::ApsLastChannelFailureRate(_) => 0xcb,
            AIBAttribute::ApsChannelTimer(_) => 0xcc,
            AIBAttribute::ApsMaxWindowSize(_) => 0xcd,
            AIBAttribute::ApsParentAnnounceTimer(_) => 0xce,
            // 0x0500 to 0x05ff reserved for custom AIBs
        }
    }
    pub fn length(&self) -> u8 {
        match self {
            AIBAttribute::ApsBindingTable => 0u8,
            _ => 0u8,
        }
    }
    pub fn value(&self) -> [u8; 8] {
        match self {
            AIBAttribute::ApsUseExtendedPanId(id) => id.to_le_bytes(),
            _ => [0u8; 8],
        }
    }
}
