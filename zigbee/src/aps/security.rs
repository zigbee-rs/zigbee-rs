//! APS layer security services (§4.4)
//!
//! Security operations (APS frame encryption, command exchange) are owned by
//! the APSME (`apsme::Apsme`). Higher-level orchestration (transport-key
//! polling, TC link-key exchange) lives in the ZDO Security Manager
//! (`zdo::ZigbeeDevice`).
