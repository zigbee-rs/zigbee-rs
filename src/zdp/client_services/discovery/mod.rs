#![allow(dead_code)]

use heapless::Vec;

use crate::apl::descriptors::node_descriptor::MacCapabilities;
use crate::apl::descriptors::node_descriptor::NodeDescriptor;
use crate::apl::descriptors::node_descriptor::ServerMask;
use crate::internal::types::IeeeAddress;
use crate::internal::types::NwkAddress;

const CLUSTER_LIST_SIZE: usize = 2 * 0xffff;

/// 2.4.3.1.1 NWK_addr_req
pub struct NWKAddrReq {
    /// The IEEE address to be matched by the Remote Device
    ieee_address: IeeeAddress,
    /// Request type for this command:
    /// 0x00 – Single device response
    /// 0x01 – Extended response
    /// 0x02-0xFF – reserved
    request_type: u8,
    /// If the Request type for this command is Extended
    /// response, the StartIndex provides the starting index
    /// for the requested elements of the associated devices list.
    start_index: u8,
}

/// 2.4.3.1.2 IEEE_addr_req
pub struct IeeeAddrReq {
    /// NWK address that is used for IEEE address mapping.
    nwk_addr_of_interest: NwkAddress,
    /// Request type for this command:
    /// 0x00 – Single device response
    /// 0x01 – Extended response
    /// 0x02-0xFF – reserved
    request_type: u8,
    /// If the Request type for this command is Extended
    /// response, the StartIndex provides the starting index
    /// for the requested elements of the associated devices list.
    start_index: u8,
}

/// 2.4.3.1.3 Node_Desc_req
pub struct NodeDescReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
}

/// 2.4.3.1.4 Power_Desc_req
pub struct PowerDescReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
}

/// 2.4.3.1.5 Simple_Desc_req
pub struct SimpleDescReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
    /// The endpoint on the destination
    endpoint: u8,
}

/// 2.4.3.1.6 Active_EP_req
pub struct ActivePeReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
}

/// 2.4.3.1.7 Match_Desc_req
pub struct MatchDescReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
    /// Profile ID to be matched at the destination.
    profile_id: u16,
    /// The number of Input Clusters provided for matching within the
    /// InClusterList.
    num_in_clusters: u8,
    /// List of Input ClusterIDs to be used for matching;
    /// the InClusterList is the desired list to be matched
    /// by the Remote Device (the elements of the InClusterList
    /// are the supported output clusters of the Local Device).
    in_cluster_list: Vec<u16, CLUSTER_LIST_SIZE>,
    /// The number of Output Clusters provided for matching within
    /// OutClusterList.
    num_out_clusters: u8,
    /// List of Output ClusterIDs to be used for matching;
    /// the OutClusterList is the desired list to be
    /// matched by the Remote Device (the elements of
    /// the OutClusterList are the supported input clusters
    /// of the Local Device).
    out_cluster_list: Vec<u16, CLUSTER_LIST_SIZE>,
}

/// 2.4.3.1.8 Complex_Desc_req
pub struct ComplexDescReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
}

/// 2.4.3.1.9 User_Desc_req
pub struct UserDescReq {
    /// NWK address for the request
    nwk_addr_of_interest: NwkAddress,
}

/// 2.4.3.1.11 Device_annce
pub struct DeviceAnnce {
    /// NWK address for the Local Device
    nwk_addr: NwkAddress,
    /// IEEE address for the Local Device
    ieee_addr: IeeeAddress,
    /// Capability of the local device
    capability: MacCapabilities,
}

/// 2.4.3.1.11 Parent_annce
pub struct ChildInfo(IeeeAddress);

pub struct ParentAnnce {
    number_of_children: u8,
    children: Vec<ChildInfo, { 255 * size_of::<ChildInfo>() }>,
}

// 2.4.3.1.13 User_Desc_set
pub struct UserDescSet {
    /// NWK address for the request.
    nwk_addr_of_interest: NwkAddress,
    /// Length of the User Descriptor in bytes.
    length: u8,
    /// The user description to configure; if the ASCII character string to be
    /// entered here is less than 16 characters in length, it shall be
    /// padded with space characters (0x20) to make a total length of 16
    /// characters. Characters with codes 0x00-0x1f are not permitted.
    user_description: Vec<u8, 255>,
}

/// 2.4.3.1.14 System_Server_Discovery_req
pub struct SystemServerDiscoveryReq {
    server_mask: ServerMask,
}

/// 2.4.3.1.15 Discovery_store_req
pub struct DiscoveryStoreReq {
    /// NWK Address for the Local Device.
    nwk_addr: NwkAddress,
    /// IEEE Address for the Local Device.
    ieee_addr: IeeeAddress,
    /// Size in bytes of the Node Descriptor for the Local Device.
    node_desc_size: u8,
    /// Size in bytes of the Power Descriptor for the Local Device.
    power_desc_size: u8,
    /// Size in bytes of the ActiveEPCount and ActiveEPList fields of the
    /// Active_EP_rsp for the Local Device.
    active_ep_size: u8,
    /// Number of Simple Descriptors supported by the Local Device (should be
    /// the same value as the ActiveEPSize).
    simple_desc_count: u8,
    /// List of bytes of simple_desc_count length, each of which represents the
    /// size in bytes of the Simple Descriptor for each Active Endpoint on
    /// the Local Device.
    simple_desc_size_list: Vec<u8, 255>,
}

/// 2.4.3.1.16 Node_Desc_store_req
pub struct NodeDescStoreReq {
    /// NWK Address for the Local Device
    nwk_addr: NwkAddress,
    /// IEEE Address for the Local Device.
    ieee_addr: IeeeAddress,
    // Node Descriptor
    node_descriptor: NodeDescriptor,
}
