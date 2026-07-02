//! Zigbee application profile identifiers.
//!
//! The profile ID identifies the application domain the cluster belongs to
//! and is carried in every APS data frame addressed by endpoint. Values are
//! allocated by the Zigbee Alliance; the set below covers the public
//! profiles in current use and is not exhaustive.
//!
//! See Zigbee R22 §2.3.2 (PDU Format) and the public Zigbee profile ID
//! registry.

/// Zigbee Device Profile (ZDP) — used by the ZDO on endpoint 0.
pub const ZDP: u16 = 0x0000;

/// Industrial Plant Monitoring.
pub const IPM: u16 = 0x0101;

/// Home Automation (ZHA).
pub const HOME_AUTOMATION: u16 = 0x0104;

/// Commercial Building Automation.
pub const COMMERCIAL_BUILDING_AUTOMATION: u16 = 0x0105;

/// Telecom Applications.
pub const TELECOM_APPLICATIONS: u16 = 0x0107;

/// Personal Home & Hospital Care.
pub const PERSONAL_HOME_AND_HOSPITAL_CARE: u16 = 0x0108;

/// Smart Energy (SE).
pub const SMART_ENERGY: u16 = 0x0109;

/// Green Power.
pub const GREEN_POWER: u16 = 0xa1e0;

/// Zigbee Light Link (ZLL).
pub const LIGHT_LINK: u16 = 0xc05e;
