//! Device descriptor configuration and ZDP discovery responses.
//!
//! Holds the node/power/endpoint descriptors this device advertises and builds
//! the matching ZDP `*_rsp` payloads (2.4.4) used to answer the discovery
//! requests a coordinator issues during interview (Node_Desc, Power_Desc,
//! Active_EP, Simple_Desc, Match_Desc, IEEE_addr, NWK_addr).

use byte::BytesExt;
use byte::ctx;
use zigbee_types::IeeeAddress;

use crate::apl::descriptors::node_descriptor::LogicalType;
use crate::nwk::nlme::management::NlmeLeaveStatus;

// ZDP request cluster identifiers (2.4.3).
pub const NWK_ADDR_REQ: u16 = 0x0000;
pub const IEEE_ADDR_REQ: u16 = 0x0001;
pub const NODE_DESC_REQ: u16 = 0x0002;
pub const POWER_DESC_REQ: u16 = 0x0003;
pub const SIMPLE_DESC_REQ: u16 = 0x0004;
pub const ACTIVE_EP_REQ: u16 = 0x0005;
pub const MATCH_DESC_REQ: u16 = 0x0006;
pub const BIND_REQ: u16 = 0x0021;
pub const UNBIND_REQ: u16 = 0x0022;
pub const MGMT_LEAVE_REQ: u16 = 0x0034;

// ZDP response cluster identifiers (2.4.4): request id | 0x8000.
pub const NWK_ADDR_RSP: u16 = 0x8000;
pub const IEEE_ADDR_RSP: u16 = 0x8001;
pub const NODE_DESC_RSP: u16 = 0x8002;
pub const POWER_DESC_RSP: u16 = 0x8003;
pub const SIMPLE_DESC_RSP: u16 = 0x8004;
pub const ACTIVE_EP_RSP: u16 = 0x8005;
pub const MATCH_DESC_RSP: u16 = 0x8006;
pub const BIND_RSP: u16 = 0x8021;
pub const UNBIND_RSP: u16 = 0x8022;
pub const MGMT_LEAVE_RSP: u16 = 0x8034;

// ZDP status codes (2.4.5)
const STATUS_SUCCESS: u8 = 0x00;
const STATUS_INV_REQUESTTYPE: u8 = 0x80;
const STATUS_DEVICE_NOT_FOUND: u8 = 0x81;
const STATUS_INVALID_EP: u8 = 0x82;
const STATUS_NOT_ACTIVE: u8 = 0x83;
const STATUS_NOT_SUPPORTED: u8 = 0x84;
const STATUS_NO_ENTRY: u8 = 0x88;
const STATUS_NOT_PERMITTED: u8 = 0x8b;
const STATUS_TABLE_FULL: u8 = 0x8c;
const STATUS_NOT_AUTHORIZED: u8 = 0x8d;

// Mgmt_Leave_req options (2.4.3.3.5)
const MGMT_LEAVE_REMOVE_CHILDREN: u8 = 0b0100_0000;
const MGMT_LEAVE_REJOIN: u8 = 0b1000_0000;

const NODE_DESCRIPTOR_SIZE: usize = 13;
const POWER_DESCRIPTOR_SIZE: usize = 2;

// a Match_Desc_req profile of 0xffff matches every endpoint (2.4.4.2.7.1.1)
const WILDCARD_PROFILE_ID: u16 = 0xffff;

/// Static configuration of the node descriptor (2.3.2.3).
#[derive(Debug, Clone, Copy)]
pub struct NodeDescriptorConfig {
    pub logical_type: LogicalType,
    pub complex_descriptor_available: bool,
    pub user_descriptor_available: bool,
    /// Frequency band field (5 bits, 2.3.2.3.5).
    pub frequency_band: u8,
    /// MAC capability flags (2.3.2.3.6) — bit 3 set means rx-on-when-idle.
    pub mac_capability_flags: u8,
    pub manufacturer_code: u16,
    pub maximum_buffer_size: u8,
    pub maximum_incoming_transfer_size: u16,
    pub server_mask: u16,
    pub maximum_outgoing_transfer_size: u16,
    pub descriptor_capability_field: u8,
}

impl NodeDescriptorConfig {
    // serialize the 13-byte node descriptor (2.3.2.3) in transmission order
    fn write(&self, out: &mut [u8]) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        let logical_type = match self.logical_type {
            LogicalType::Coordinator => 0b000,
            LogicalType::Router => 0b001,
            LogicalType::EndDevice => 0b010,
            LogicalType::Reserved(v) => v,
        };
        let byte0 = (logical_type & 0b111)
            | (u8::from(self.complex_descriptor_available) << 3)
            | (u8::from(self.user_descriptor_available) << 4);
        out.write_with(offset, byte0, ctx::LE)?;
        // APS flags (3 bits, always 0) + frequency band (5 bits)
        out.write_with(offset, (self.frequency_band & 0x1f) << 3, ctx::LE)?;
        out.write_with(offset, self.mac_capability_flags, ctx::LE)?;
        out.write_with(offset, self.manufacturer_code, ctx::LE)?;
        out.write_with(offset, self.maximum_buffer_size, ctx::LE)?;
        out.write_with(offset, self.maximum_incoming_transfer_size, ctx::LE)?;
        out.write_with(offset, self.server_mask, ctx::LE)?;
        out.write_with(offset, self.maximum_outgoing_transfer_size, ctx::LE)?;
        out.write_with(offset, self.descriptor_capability_field, ctx::LE)?;
        Ok(*offset)
    }
}

/// Static configuration of the node power descriptor (2.3.2.4).
#[derive(Debug, Clone, Copy)]
pub struct PowerDescriptorConfig {
    /// Sleep/power-saving mode (2.3.2.4.1): `0x0` receiver on when idle,
    /// `0x1` periodically on, `0x2` on when stimulated.
    pub current_power_mode: u8,
    /// Bitmap of the sources present (2.3.2.4.2): bit 0 constant/mains,
    /// bit 1 rechargeable battery, bit 2 disposable battery.
    pub available_power_sources: u8,
    /// Bitmap (same bit assignment) with the source currently in use set
    /// (2.3.2.4.3).
    pub current_power_source: u8,
    /// Charge level (2.3.2.4.4): `0b0000` critical, `0b0100` 33%,
    /// `0b1000` 66%, `0b1100` 100%.
    pub current_power_source_level: u8,
}

impl PowerDescriptorConfig {
    // serialize the 2-byte node power descriptor (2.3.2.4); each octet holds
    // the earlier-transmitted field in its low nibble
    fn write(&self, out: &mut [u8]) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        out.write_with(
            offset,
            (self.current_power_mode & 0x0f) | ((self.available_power_sources & 0x0f) << 4),
            ctx::LE,
        )?;
        out.write_with(
            offset,
            (self.current_power_source & 0x0f) | ((self.current_power_source_level & 0x0f) << 4),
            ctx::LE,
        )?;
        Ok(*offset)
    }
}

/// One application endpoint exposed by this device (2.3.2.5).
#[derive(Debug, Clone, Copy)]
pub struct EndpointDescriptor<'a> {
    pub endpoint: u8,
    pub profile_id: u16,
    pub device_id: u16,
    pub device_version: u8,
    pub input_clusters: &'a [u16],
    pub output_clusters: &'a [u16],
}

impl EndpointDescriptor<'_> {
    // serialize the simple descriptor (2.3.2.5); cluster ids as little-endian u16
    fn write(&self, out: &mut [u8]) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        out.write_with(offset, self.endpoint, ctx::LE)?;
        out.write_with(offset, self.profile_id, ctx::LE)?;
        out.write_with(offset, self.device_id, ctx::LE)?;
        out.write_with(offset, self.device_version & 0x0f, ctx::LE)?;
        out.write_with(
            offset,
            u8::try_from(self.input_clusters.len()).unwrap_or(u8::MAX),
            ctx::LE,
        )?;
        for &cluster in self.input_clusters {
            out.write_with(offset, cluster, ctx::LE)?;
        }
        out.write_with(
            offset,
            u8::try_from(self.output_clusters.len()).unwrap_or(u8::MAX),
            ctx::LE,
        )?;
        for &cluster in self.output_clusters {
            out.write_with(offset, cluster, ctx::LE)?;
        }
        Ok(*offset)
    }
}

/// The descriptors this device advertises to the network.
#[derive(Debug, Clone, Copy)]
pub struct DeviceDescriptorConfig<'a> {
    pub node: NodeDescriptorConfig,
    pub power: PowerDescriptorConfig,
    pub endpoints: &'a [EndpointDescriptor<'a>],
}

impl DeviceDescriptorConfig<'_> {
    fn endpoint(&self, endpoint: u8) -> Option<&EndpointDescriptor<'_>> {
        self.endpoints.iter().find(|e| e.endpoint == endpoint)
    }

    /// Whether the device implements the given endpoint.
    pub fn supports_endpoint(&self, endpoint: u8) -> bool {
        self.endpoint(endpoint).is_some()
    }

    /// Build a Node_Desc_rsp payload (2.4.4.2.3): seq, status,
    /// NWKAddrOfInterest, node descriptor. Returns bytes written.
    pub fn node_desc_rsp(
        &self,
        seq: u8,
        nwk_addr: u16,
        out: &mut [u8],
    ) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        out.write_with(offset, seq, ctx::LE)?;
        out.write_with(offset, STATUS_SUCCESS, ctx::LE)?;
        out.write_with(offset, nwk_addr, ctx::LE)?;
        let mut nd = [0u8; NODE_DESCRIPTOR_SIZE];
        let n = self.node.write(&mut nd)?;
        out[*offset..*offset + n].copy_from_slice(&nd[..n]);
        *offset += n;
        Ok(*offset)
    }

    /// Build a Power_Desc_rsp payload (2.4.4.2.4): seq, status,
    /// NWKAddrOfInterest, power descriptor.
    pub fn power_desc_rsp(
        &self,
        seq: u8,
        nwk_addr: u16,
        out: &mut [u8],
    ) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        out.write_with(offset, seq, ctx::LE)?;
        out.write_with(offset, STATUS_SUCCESS, ctx::LE)?;
        out.write_with(offset, nwk_addr, ctx::LE)?;
        let mut pd = [0u8; POWER_DESCRIPTOR_SIZE];
        let n = self.power.write(&mut pd)?;
        out[*offset..*offset + n].copy_from_slice(&pd[..n]);
        *offset += n;
        Ok(*offset)
    }

    /// Endpoints matching the request (2.4.4.2.7.1.1), written into `out`.
    ///
    /// Returns how many were written.
    pub fn matching_endpoints(&self, request: &MatchDescReq<'_>, out: &mut [u8]) -> usize {
        let mut count = 0;
        for ep in self.endpoints {
            if request.profile_id != WILDCARD_PROFILE_ID && request.profile_id != ep.profile_id {
                continue;
            }
            let matches = request
                .input_clusters()
                .any(|cluster| ep.input_clusters.contains(&cluster))
                || request
                    .output_clusters()
                    .any(|cluster| ep.output_clusters.contains(&cluster));
            if matches {
                let Some(slot) = out.get_mut(count) else {
                    break;
                };
                *slot = ep.endpoint;
                count += 1;
            }
        }
        count
    }

    /// Build an Active_EP_rsp payload (2.4.4.2.6): seq, status,
    /// NWKAddrOfInterest, endpoint count + list.
    pub fn active_ep_rsp(
        &self,
        seq: u8,
        nwk_addr: u16,
        out: &mut [u8],
    ) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        out.write_with(offset, seq, ctx::LE)?;
        out.write_with(offset, STATUS_SUCCESS, ctx::LE)?;
        out.write_with(offset, nwk_addr, ctx::LE)?;
        out.write_with(
            offset,
            u8::try_from(self.endpoints.len()).unwrap_or(u8::MAX),
            ctx::LE,
        )?;
        for ep in self.endpoints {
            out.write_with(offset, ep.endpoint, ctx::LE)?;
        }
        Ok(*offset)
    }

    /// Build a Simple_Desc_rsp payload (2.4.4.2.5): seq, status,
    /// NWKAddrOfInterest, length, simple descriptor. An unknown endpoint yields
    /// status NOT_ACTIVE with length 0.
    pub fn simple_desc_rsp(
        &self,
        seq: u8,
        nwk_addr: u16,
        endpoint: u8,
        out: &mut [u8],
    ) -> Result<usize, byte::Error> {
        let offset = &mut 0;
        out.write_with(offset, seq, ctx::LE)?;

        if let Some(ep) = self.endpoint(endpoint) {
            let mut desc = [0u8; 64];
            let len = ep.write(&mut desc)?;
            out.write_with(offset, STATUS_SUCCESS, ctx::LE)?;
            out.write_with(offset, nwk_addr, ctx::LE)?;
            out.write_with(offset, u8::try_from(len).unwrap_or(u8::MAX), ctx::LE)?;
            out[*offset..*offset + len].copy_from_slice(&desc[..len]);
            *offset += len;
        } else {
            out.write_with(offset, STATUS_NOT_ACTIVE, ctx::LE)?;
            out.write_with(offset, nwk_addr, ctx::LE)?;
            out.write_with(offset, 0u8, ctx::LE)?;
        }
        Ok(*offset)
    }
}

/// Build an IEEE_addr_rsp / NWK_addr_rsp payload (2.4.4.1.1/2.4.4.1.2) for a
/// single-device (request type 0x00) response: seq, status, IEEEAddrRemoteDev,
/// NWKAddrRemoteDev.
pub fn addr_rsp(
    seq: u8,
    ieee_addr: IeeeAddress,
    nwk_addr: u16,
    out: &mut [u8],
) -> Result<usize, byte::Error> {
    let offset = &mut 0;
    out.write_with(offset, seq, ctx::LE)?;
    out.write_with(offset, STATUS_SUCCESS, ctx::LE)?;
    out.write_with(offset, ieee_addr.0, ctx::LE)?;
    out.write_with(offset, nwk_addr, ctx::LE)?;
    Ok(*offset)
}

/// A parsed Bind_req / Unbind_req (2.4.3.2.2/2.4.3.2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindReq {
    /// ZDP transaction sequence number to echo in the response.
    pub seq: u8,
    /// Device the binding applies to; must be this device.
    pub src_address: IeeeAddress,
    pub src_endpoint: u8,
    pub cluster_id: u16,
    pub destination: BindReqDestination,
}

/// Destination of a Bind_req, by addressing mode (2.4.3.2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindReqDestination {
    Group(u16),
    Device { ieee: IeeeAddress, endpoint: u8 },
}

/// Parse a Bind_req / Unbind_req payload (2.4.3.2.2): seq, SrcAddress(8),
/// SrcEndp(1), ClusterID(2), DstAddrMode(1), DstAddress(2 or 8), DstEndp(0/1).
pub fn parse_bind_req(asdu: &[u8]) -> Option<BindReq> {
    let offset = &mut 0;
    let seq: u8 = asdu.read_with(offset, ctx::LE).ok()?;
    let src_address: u64 = asdu.read_with(offset, ctx::LE).ok()?;
    let src_endpoint: u8 = asdu.read_with(offset, ctx::LE).ok()?;
    let cluster_id: u16 = asdu.read_with(offset, ctx::LE).ok()?;
    let dst_addr_mode: u8 = asdu.read_with(offset, ctx::LE).ok()?;

    let destination = match dst_addr_mode {
        BIND_ADDR_MODE_GROUP => BindReqDestination::Group(asdu.read_with(offset, ctx::LE).ok()?),
        BIND_ADDR_MODE_DEVICE => BindReqDestination::Device {
            ieee: IeeeAddress(asdu.read_with(offset, ctx::LE).ok()?),
            endpoint: asdu.read_with(offset, ctx::LE).ok()?,
        },
        _ => return None,
    };

    Some(BindReq {
        seq,
        src_address: IeeeAddress(src_address),
        src_endpoint,
        cluster_id,
        destination,
    })
}

/// Bind_req destination addressing mode: 16-bit group address (2.4.3.2.2).
const BIND_ADDR_MODE_GROUP: u8 = 0x01;
/// Bind_req destination addressing mode: 64-bit address plus endpoint.
const BIND_ADDR_MODE_DEVICE: u8 = 0x03;

/// Outcome of a Bind_req / Unbind_req, as reported in the response
/// (2.4.4.3.2/2.4.4.3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindStatus {
    Success,
    InvalidEndpoint,
    TableFull,
    NoEntry,
    NotSupported,
}

impl BindStatus {
    fn code(self) -> u8 {
        match self {
            Self::Success => STATUS_SUCCESS,
            Self::InvalidEndpoint => STATUS_INVALID_EP,
            Self::TableFull => STATUS_TABLE_FULL,
            Self::NoEntry => STATUS_NO_ENTRY,
            Self::NotSupported => STATUS_NOT_SUPPORTED,
        }
    }
}

/// Build a Bind_rsp / Unbind_rsp payload (2.4.4.3.2/2.4.4.3.3): seq, status.
pub fn bind_rsp(seq: u8, status: BindStatus, out: &mut [u8]) -> Result<usize, byte::Error> {
    let offset = &mut 0;
    out.write_with(offset, seq, ctx::LE)?;
    out.write_with(offset, status.code(), ctx::LE)?;
    Ok(*offset)
}

/// A parsed Match_Desc_req (2.4.3.1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchDescReq<'a> {
    /// ZDP transaction sequence number to echo in the response.
    pub seq: u8,
    pub nwk_addr_of_interest: u16,
    pub profile_id: u16,
    input_clusters: &'a [u8],
    output_clusters: &'a [u8],
}

impl MatchDescReq<'_> {
    /// Input cluster identifiers to match against.
    pub fn input_clusters(&self) -> impl Iterator<Item = u16> + '_ {
        cluster_ids(self.input_clusters)
    }

    /// Output cluster identifiers to match against.
    pub fn output_clusters(&self) -> impl Iterator<Item = u16> + '_ {
        cluster_ids(self.output_clusters)
    }
}

fn cluster_ids(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(u16::from_le_bytes)
}

/// Parse a Match_Desc_req payload (2.4.3.1.7).
///
/// Returns `None` if the payload is truncated or a declared cluster count runs
/// past the end of the ASDU.
pub fn parse_match_desc_req(asdu: &[u8]) -> Option<MatchDescReq<'_>> {
    let seq = *asdu.first()?;
    let nwk_addr_of_interest = u16::from_le_bytes(asdu.get(1..3)?.try_into().ok()?);
    let profile_id = u16::from_le_bytes(asdu.get(3..5)?.try_into().ok()?);

    let in_count = usize::from(*asdu.get(5)?);
    let in_end = 6 + in_count * 2;
    let input_clusters = asdu.get(6..in_end)?;

    let out_count = usize::from(*asdu.get(in_end)?);
    let out_start = in_end + 1;
    let output_clusters = asdu.get(out_start..out_start + out_count * 2)?;

    Some(MatchDescReq {
        seq,
        nwk_addr_of_interest,
        profile_id,
        input_clusters,
        output_clusters,
    })
}

/// Build a Match_Desc_rsp payload (2.4.4.2.7): seq, status,
/// NWKAddrOfInterest, MatchLength, MatchList.
pub fn match_desc_rsp(
    seq: u8,
    nwk_addr: u16,
    matches: &[u8],
    out: &mut [u8],
) -> Result<usize, byte::Error> {
    let offset = &mut 0;
    out.write_with(offset, seq, ctx::LE)?;
    out.write_with(offset, STATUS_SUCCESS, ctx::LE)?;
    out.write_with(offset, nwk_addr, ctx::LE)?;
    out.write_with(
        offset,
        u8::try_from(matches.len()).unwrap_or(u8::MAX),
        ctx::LE,
    )?;
    for &endpoint in matches {
        out.write_with(offset, endpoint, ctx::LE)?;
    }
    Ok(*offset)
}

/// Build a Match_Desc_rsp carrying only Status and MatchLength (2.4.4.2.7
/// step 2.b.iii), for a unicast request about another device.
///
/// NWKAddrOfInterest is omitted on purpose: the error paths of 2.4.4.2.7
/// (steps 2.b.iii, 4.b.ii, 6.b) all specify Status and MatchLength only, and
/// only the success path (step 7.b) carries the address. Dissectors that model
/// the response as a fixed layout report the short frame as malformed.
pub fn match_desc_rsp_invalid_request(seq: u8, out: &mut [u8]) -> Result<usize, byte::Error> {
    let offset = &mut 0;
    out.write_with(offset, seq, ctx::LE)?;
    out.write_with(offset, STATUS_INV_REQUESTTYPE, ctx::LE)?;
    out.write_with(offset, 0u8, ctx::LE)?;
    Ok(*offset)
}

/// A parsed Mgmt_Leave_req (2.4.3.3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MgmtLeaveReq {
    /// ZDP transaction sequence number to echo in the response.
    pub seq: u8,
    /// Device to remove; `None` for the NULL address, which targets the
    /// recipient itself.
    pub device_address: Option<IeeeAddress>,
    pub remove_children: bool,
    pub rejoin: bool,
}

/// Parse a Mgmt_Leave_req payload (2.4.3.3.5).
///
/// Returns `None` if the payload is too short.
pub fn parse_mgmt_leave_req(asdu: &[u8]) -> Option<MgmtLeaveReq> {
    let seq = *asdu.first()?;
    let addr_bytes: [u8; 8] = asdu.get(1..9)?.try_into().ok()?;
    let device_address = match u64::from_le_bytes(addr_bytes) {
        0 => None,
        addr => Some(IeeeAddress(addr)),
    };
    let options = *asdu.get(9)?;
    Some(MgmtLeaveReq {
        seq,
        device_address,
        remove_children: options & MGMT_LEAVE_REMOVE_CHILDREN != 0,
        rejoin: options & MGMT_LEAVE_REJOIN != 0,
    })
}

/// Map an NLME-LEAVE.confirm status onto the ZDP status enumeration (2.4.5).
///
/// The Mgmt_Leave_rsp Status field carries NOT_SUPPORTED, NOT_AUTHORIZED or
/// any status code returned by NLME-LEAVE.confirm (2.4.4.4.5).
pub fn leave_status(status: NlmeLeaveStatus) -> u8 {
    match status {
        NlmeLeaveStatus::Success => STATUS_SUCCESS,
        NlmeLeaveStatus::InvalidRequest => STATUS_INV_REQUESTTYPE,
        NlmeLeaveStatus::UnknownDevice => STATUS_DEVICE_NOT_FOUND,
        NlmeLeaveStatus::MacError => STATUS_NOT_PERMITTED,
        NlmeLeaveStatus::NotAuthorized => STATUS_NOT_AUTHORIZED,
    }
}

/// Build a Mgmt_Leave_rsp payload (2.4.4.4.5): seq, status.
pub fn mgmt_leave_rsp(seq: u8, status: u8, out: &mut [u8]) -> Result<usize, byte::Error> {
    let offset = &mut 0;
    out.write_with(offset, seq, ctx::LE)?;
    out.write_with(offset, status, ctx::LE)?;
    Ok(*offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODE: NodeDescriptorConfig = NodeDescriptorConfig {
        logical_type: LogicalType::EndDevice,
        complex_descriptor_available: false,
        user_descriptor_available: false,
        frequency_band: 0x08,
        mac_capability_flags: 0x88,
        manufacturer_code: 0x1037,
        maximum_buffer_size: 80,
        maximum_incoming_transfer_size: 128,
        server_mask: 0,
        maximum_outgoing_transfer_size: 128,
        descriptor_capability_field: 0,
    };

    // mains-powered, on when idle, 100% charge
    const POWER: PowerDescriptorConfig = PowerDescriptorConfig {
        current_power_mode: 0b0000,
        available_power_sources: 0b0001,
        current_power_source: 0b0001,
        current_power_source_level: 0b1100,
    };

    const ENDPOINTS: [EndpointDescriptor; 1] = [EndpointDescriptor {
        endpoint: 1,
        profile_id: 0x0104,
        device_id: 0x0302,
        device_version: 1,
        input_clusters: &[0x0000, 0x0402],
        output_clusters: &[0x0019],
    }];

    fn cfg() -> DeviceDescriptorConfig<'static> {
        DeviceDescriptorConfig {
            node: NODE,
            power: POWER,
            endpoints: &ENDPOINTS,
        }
    }

    #[test]
    fn node_desc_rsp_layout() {
        let mut out = [0u8; 32];
        let n = cfg().node_desc_rsp(0x42, 0x1234, &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &[
                0x42, 0x00, 0x34, 0x12, // seq, status, NWKAddrOfInterest
                0x02, 0x40, 0x88, // logical type / freq band / mac flags
                0x37, 0x10, // manufacturer code
                0x50, // max buffer
                0x80, 0x00, // max incoming
                0x00, 0x00, // server mask
                0x80, 0x00, // max outgoing
                0x00, // descriptor capability
            ]
        );
    }

    #[test]
    fn active_ep_rsp_layout() {
        let mut out = [0u8; 16];
        let n = cfg().active_ep_rsp(0x07, 0x1234, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x07, 0x00, 0x34, 0x12, 0x01, 0x01]);
    }

    #[test]
    fn simple_desc_rsp_known_endpoint() {
        let mut out = [0u8; 32];
        let n = cfg().simple_desc_rsp(0x07, 0x1234, 1, &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &[
                0x07, 0x00, 0x34, 0x12, // seq, status, NWKAddrOfInterest
                0x0e, // length
                0x01, // endpoint
                0x04, 0x01, // profile id
                0x02, 0x03, // device id
                0x01, // device version
                0x02, 0x00, 0x00, 0x02, 0x04, // input count + clusters
                0x01, 0x19, 0x00, // output count + clusters
            ]
        );
    }

    #[test]
    fn simple_desc_rsp_unknown_endpoint_not_active() {
        let mut out = [0u8; 16];
        let n = cfg().simple_desc_rsp(0x07, 0x1234, 9, &mut out).unwrap();
        // status NOT_ACTIVE (0x83), length 0, no descriptor
        assert_eq!(&out[..n], &[0x07, 0x83, 0x34, 0x12, 0x00]);
    }

    #[test]
    fn power_desc_rsp_layout() {
        let mut out = [0u8; 16];
        let n = cfg().power_desc_rsp(0x07, 0x1234, &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &[
                0x07, 0x00, 0x34, 0x12, // seq, status, NWKAddrOfInterest
                0x10, // current power mode | available sources << 4
                0xc1, // current source | level << 4
            ]
        );
    }

    // 2.4.3.1.7: seq, NWKAddrOfInterest, ProfileID, then both cluster lists
    const MATCH_REQ: [u8; 13] = [
        0x33, // seq
        0x34, 0x12, // NWKAddrOfInterest
        0x04, 0x01, // profile id 0x0104
        0x02, 0x00, 0x00, 0x02, 0x04, // 2 input clusters: 0x0000, 0x0402
        0x01, 0x19, 0x00, // 1 output cluster: 0x0019
    ];

    #[test]
    fn parse_match_desc_req_lists() {
        let req = parse_match_desc_req(&MATCH_REQ).unwrap();
        assert_eq!(req.seq, 0x33);
        assert_eq!(req.nwk_addr_of_interest, 0x1234);
        assert_eq!(req.profile_id, 0x0104);
        assert!(req.input_clusters().eq([0x0000, 0x0402]));
        assert!(req.output_clusters().eq([0x0019]));
    }

    #[test]
    fn parse_match_desc_req_rejects_truncated_list() {
        // declares two input clusters but carries one and a half
        assert!(parse_match_desc_req(&MATCH_REQ[..8]).is_none());
        // missing the output cluster count entirely
        assert!(parse_match_desc_req(&MATCH_REQ[..10]).is_none());
        assert!(parse_match_desc_req(&[0x33, 0x34]).is_none());
    }

    #[test]
    fn matching_endpoints_by_input_cluster() {
        // temperature measurement is an input cluster on endpoint 1
        let asdu = [0x33, 0x34, 0x12, 0x04, 0x01, 0x01, 0x02, 0x04, 0x00];
        let req = parse_match_desc_req(&asdu).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(cfg().matching_endpoints(&req, &mut out), 1);
        assert_eq!(out[0], 1);
    }

    #[test]
    fn matching_endpoints_by_output_cluster() {
        // OTA (0x0019) is an output cluster on endpoint 1
        let asdu = [0x33, 0x34, 0x12, 0x04, 0x01, 0x00, 0x01, 0x19, 0x00];
        let req = parse_match_desc_req(&asdu).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(cfg().matching_endpoints(&req, &mut out), 1);
    }

    #[test]
    fn matching_endpoints_wildcard_profile() {
        // profile 0xffff matches regardless of the endpoint's profile
        let asdu = [0x33, 0x34, 0x12, 0xff, 0xff, 0x01, 0x02, 0x04, 0x00];
        let req = parse_match_desc_req(&asdu).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(cfg().matching_endpoints(&req, &mut out), 1);
    }

    #[test]
    fn matching_endpoints_rejects_wrong_profile_and_unknown_cluster() {
        // right cluster, wrong profile
        let asdu = [0x33, 0x34, 0x12, 0x09, 0x01, 0x01, 0x02, 0x04, 0x00];
        let req = parse_match_desc_req(&asdu).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(cfg().matching_endpoints(&req, &mut out), 0);

        // right profile, cluster we do not serve
        let asdu = [0x33, 0x34, 0x12, 0x04, 0x01, 0x01, 0x06, 0x00, 0x00];
        let req = parse_match_desc_req(&asdu).unwrap();
        assert_eq!(cfg().matching_endpoints(&req, &mut out), 0);
    }

    #[test]
    fn match_desc_rsp_layout() {
        let mut out = [0u8; 16];
        let n = match_desc_rsp(0x33, 0x1234, &[1], &mut out).unwrap();
        assert_eq!(&out[..n], &[0x33, 0x00, 0x34, 0x12, 0x01, 0x01]);
    }

    #[test]
    fn match_desc_rsp_invalid_request_omits_address() {
        let mut out = [0u8; 16];
        let n = match_desc_rsp_invalid_request(0x33, &mut out).unwrap();
        // seq, INV_REQUESTTYPE, MatchLength — no NWKAddrOfInterest (2.4.4.2.7
        // step 2.b.iii)
        assert_eq!(&out[..n], &[0x33, 0x80, 0x00]);
    }

    #[test]
    fn bind_rsp_carries_status() {
        let mut out = [0u8; 4];
        let n = bind_rsp(0x21, BindStatus::Success, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x21, 0x00]);
        let n = bind_rsp(0x21, BindStatus::InvalidEndpoint, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x21, 0x82]);
        let n = bind_rsp(0x21, BindStatus::TableFull, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x21, 0x8c]);
    }

    #[test]
    fn parse_bind_req_device_and_group_modes() {
        // seq, SrcAddress, SrcEndp, ClusterID, DstAddrMode=0x03, DstAddress,
        // DstEndp
        let device = [
            0x21, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x0a, 0x02, 0x04, 0x03, 0x01,
            0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x01,
        ];
        let request = parse_bind_req(&device).unwrap();
        assert_eq!(request.seq, 0x21);
        assert_eq!(request.src_address, IeeeAddress(0x8877_6655_4433_2211));
        assert_eq!(request.src_endpoint, 0x0a);
        assert_eq!(request.cluster_id, 0x0402);
        assert_eq!(
            request.destination,
            BindReqDestination::Device {
                ieee: IeeeAddress(0x0807_0605_0403_0201),
                endpoint: 0x01,
            }
        );

        // DstAddrMode=0x01 carries a group address and no endpoint
        let group = [
            0x22, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x0a, 0x02, 0x04, 0x01, 0x34,
            0x12,
        ];
        let request = parse_bind_req(&group).unwrap();
        assert_eq!(request.destination, BindReqDestination::Group(0x1234));

        // a truncated device-mode request is rejected
        assert!(parse_bind_req(&device[..device.len() - 1]).is_none());
        // as is an unknown addressing mode
        let mut unknown = device;
        unknown[12] = 0x02;
        assert!(parse_bind_req(&unknown).is_none());
    }

    #[test]
    fn addr_rsp_layout() {
        let mut out = [0u8; 16];
        let n = addr_rsp(0x07, IeeeAddress(0x0011_2233_4455_6677), 0x1234, &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &[
                0x07, 0x00, // seq, status
                0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00, // IEEE addr (LE)
                0x34, 0x12, // NWK addr (LE)
            ]
        );
    }

    #[test]
    fn parse_mgmt_leave_req_self_null_address() {
        let asdu = [
            0x09, // seq
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,        // NULL device address
            0b1100_0000, // remove children + rejoin
        ];
        let req = parse_mgmt_leave_req(&asdu).unwrap();
        assert_eq!(req.seq, 0x09);
        assert_eq!(req.device_address, None);
        assert!(req.remove_children);
        assert!(req.rejoin);
    }

    #[test]
    fn parse_mgmt_leave_req_targets_device() {
        let asdu = [
            0x0a, // seq
            0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00, // IEEE addr (LE)
            0x00, // no flags
        ];
        let req = parse_mgmt_leave_req(&asdu).unwrap();
        assert_eq!(req.seq, 0x0a);
        assert_eq!(req.device_address, Some(IeeeAddress(0x0011_2233_4455_6677)));
        assert!(!req.remove_children);
        assert!(!req.rejoin);
    }

    #[test]
    fn parse_mgmt_leave_req_too_short() {
        assert!(parse_mgmt_leave_req(&[0x01, 0x02]).is_none());
    }

    #[test]
    fn mgmt_leave_rsp_layout() {
        let mut out = [0u8; 4];
        let n = mgmt_leave_rsp(0x09, 0x00, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x09, 0x00]);
    }
}
