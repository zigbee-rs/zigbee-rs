//! Device descriptor configuration and ZDP discovery responses.
//!
//! Holds the node/endpoint descriptors this device advertises and builds the
//! matching ZDP `*_rsp` payloads (§2.4.4) used to answer the discovery requests
//! a coordinator issues during interview (Node_Desc, Active_EP, Simple_Desc,
//! IEEE_addr, NWK_addr).

use byte::BytesExt;
use byte::ctx;
use zigbee_types::IeeeAddress;

use crate::apl::descriptors::node_descriptor::LogicalType;

// ZDP request cluster identifiers (§2.4.3).
pub const NWK_ADDR_REQ: u16 = 0x0000;
pub const IEEE_ADDR_REQ: u16 = 0x0001;
pub const NODE_DESC_REQ: u16 = 0x0002;
pub const SIMPLE_DESC_REQ: u16 = 0x0004;
pub const ACTIVE_EP_REQ: u16 = 0x0005;
pub const BIND_REQ: u16 = 0x0021;
pub const UNBIND_REQ: u16 = 0x0022;

// ZDP response cluster identifiers (§2.4.4): request id | 0x8000.
pub const NWK_ADDR_RSP: u16 = 0x8000;
pub const IEEE_ADDR_RSP: u16 = 0x8001;
pub const NODE_DESC_RSP: u16 = 0x8002;
pub const SIMPLE_DESC_RSP: u16 = 0x8004;
pub const ACTIVE_EP_RSP: u16 = 0x8005;
pub const BIND_RSP: u16 = 0x8021;
pub const UNBIND_RSP: u16 = 0x8022;

/// ZDP status: SUCCESS (§2.4.5).
const STATUS_SUCCESS: u8 = 0x00;
/// ZDP status: supplied endpoint was 0x00 or 0xff.
const STATUS_INVALID_EP: u8 = 0x82;
/// ZDP status: requested endpoint is not described by a simple descriptor.
const STATUS_NOT_ACTIVE: u8 = 0x83;

const NODE_DESCRIPTOR_SIZE: usize = 13;

/// Static configuration of the node descriptor (§2.3.2.3).
#[derive(Debug, Clone, Copy)]
pub struct NodeDescriptorConfig {
    pub logical_type: LogicalType,
    pub complex_descriptor_available: bool,
    pub user_descriptor_available: bool,
    /// Frequency band field (5 bits, §2.3.2.3.5).
    pub frequency_band: u8,
    /// MAC capability flags (§2.3.2.3.6) — bit 3 set means rx-on-when-idle.
    pub mac_capability_flags: u8,
    pub manufacturer_code: u16,
    pub maximum_buffer_size: u8,
    pub maximum_incoming_transfer_size: u16,
    pub server_mask: u16,
    pub maximum_outgoing_transfer_size: u16,
    pub descriptor_capability_field: u8,
}

impl NodeDescriptorConfig {
    /// Serialize the 13-byte node descriptor (§2.3.2.3) in transmission order.
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

/// One application endpoint exposed by this device (§2.3.2.5).
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
    /// Serialize the simple descriptor (§2.3.2.5): cluster counts are the number
    /// of clusters, each cluster id transmitted as a little-endian `u16`.
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
    pub endpoints: &'a [EndpointDescriptor<'a>],
}

impl DeviceDescriptorConfig<'_> {
    fn endpoint(&self, endpoint: u8) -> Option<&EndpointDescriptor<'_>> {
        self.endpoints.iter().find(|e| e.endpoint == endpoint)
    }

    /// Build a Node_Desc_rsp payload (§2.4.4.2.3): seq, status,
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

    /// Build an Active_EP_rsp payload (§2.4.4.2.6): seq, status,
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

    /// Build a Simple_Desc_rsp payload (§2.4.4.2.5): seq, status,
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

/// Build an IEEE_addr_rsp / NWK_addr_rsp payload (§2.4.4.1.1/§2.4.4.1.2) for a
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

/// Build a Bind_rsp / Unbind_rsp payload (§2.4.4.3.2/§2.4.4.3.3): seq, status.
/// `src_endpoint` is the SrcEndp field of the request; per §2.4.4.3.2 an
/// endpoint outside the inclusive 0x01-0xfe range yields INVALID_EP, otherwise
/// the binding is accepted with SUCCESS.
pub fn bind_rsp(seq: u8, src_endpoint: u8, out: &mut [u8]) -> Result<usize, byte::Error> {
    let status = if (0x01..=0xfe).contains(&src_endpoint) {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_EP
    };
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

    const ENDPOINTS: [EndpointDescriptor; 1] = [EndpointDescriptor {
        endpoint: 1,
        profile_id: 0x0104,
        device_id: 0x0302,
        device_version: 1,
        input_clusters: &[0x0000, 0x0402],
        output_clusters: &[],
    }];

    fn cfg() -> DeviceDescriptorConfig<'static> {
        DeviceDescriptorConfig {
            node: NODE,
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
                0x0c, // length
                0x01, // endpoint
                0x04, 0x01, // profile id
                0x02, 0x03, // device id
                0x01, // device version
                0x02, 0x00, 0x00, 0x02, 0x04, // input count + clusters
                0x00, // output count
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
    fn bind_rsp_success_for_valid_endpoint() {
        let mut out = [0u8; 4];
        let n = bind_rsp(0x21, 1, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x21, 0x00]);
    }

    #[test]
    fn bind_rsp_invalid_endpoint() {
        let mut out = [0u8; 4];
        let n = bind_rsp(0x21, 0x00, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x21, 0x82]);
        let n = bind_rsp(0x21, 0xff, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x21, 0x82]);
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
}
