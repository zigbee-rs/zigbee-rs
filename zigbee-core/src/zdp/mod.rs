//! Device Profile
//!
//! The Zigbee Device Profile operates like any Zigbee profile by defining
//! clusters. Unlike application specific profiles, the clusters within the
//! Zigbee Device Profile define capabilities supported in all Zigbee devices.
//!
//! The Device Profile supports four key inter-device communication functions
//! within the Zigbee protocol. These functions are explained in the following
//! sections:
//! * Device and Service Discovery Overview
//! * End Device Bind Overview
//! * Bind and Unbind Overview
//! * Binding Table Management Overview
//! * Network Management Overview

pub mod client_services;
pub mod device_annce;
