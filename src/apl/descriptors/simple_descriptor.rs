//! 2.3.2.5 Simple Descriptor
//!
//! The simple descriptor contains information specific to each endpoint
//! contained in this node. The simple descriptor is  mandatory for each
//! endpoint present in the node.

use heapless::Vec;

use crate::apl::descriptors::error::Error;

const MAX_CLUSTER_COUNT: usize = (16 * 255) / 8; // 510
const SIMPLE_DESCRIPTOR_SIZE: usize = 8 + 2 * MAX_CLUSTER_COUNT; // 1028

pub struct SimpleDescriptor(Vec<u8, SIMPLE_DESCRIPTOR_SIZE>);

impl SimpleDescriptor {
    fn new(
        endpoint: u8,
        application_profile_identifier: u16,
        application_device_identifier: u16,
        application_device_version: u8,
        application_input_cluster_count: u8,
        application_input_cluster_list: Option<Vec<u8, MAX_CLUSTER_COUNT>>,
        application_output_cluster_count: u8,
        application_output_cluster_list: Option<Vec<u8, MAX_CLUSTER_COUNT>>,
    ) -> Result<Self, Error> {
        todo!()
    }
}
