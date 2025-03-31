//! 2.3.2.5 Simple Descriptor
//!
//! The simple descriptor contains information specific to each endpoint contained in this node.
//! The simple descriptor is  mandatory for each endpoint present in the node.
//!

use heapless::Vec;

const MAX_CLUSTER_COUNT: usize = (16 * 255) / 8; // 510
const SIMPLE_DESCRIPTOR_SIZE: usize = 8 + 2 * MAX_CLUSTER_COUNT; // 1028

pub struct ApplicationClusterList(Vec<u8, MAX_CLUSTER_COUNT>);

pub struct SimpleDescriptor {
    endpoint: u8,
    application_profile_identifier: u16,
    application_device_identifier: u16,
    application_device_version: u8,
    application_input_cluster_count: u8,
    application_input_cluster_list: Option<ApplicationClusterList>,
    application_output_cluster_count: u8,
    application_output_cluster_list: Option<ApplicationClusterList>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_simple_descriptor_with_input_and_output_cluster_list_should_succeed() {
        // given
        let endpoint: u8 = 42;
        let application_profile_identifier: u16 = 123;
        let application_device_identifier: u16 = 456;
        let application_device_version: u8 = 5;

        let application_input_cluster_count: u8 = 2;
        let input_values: Vec<u8, MAX_CLUSTER_COUNT> =
            (0..MAX_CLUSTER_COUNT).map(|v| (v + 1) as u8).collect();
        let application_input_cluster_list: Option<ApplicationClusterList> =
            Some(ApplicationClusterList(input_values));

        let application_output_cluster_count: u8 = 1;
        let output_values: Vec<u8, MAX_CLUSTER_COUNT> =
            (0..MAX_CLUSTER_COUNT).map(|v| (v + 2) as u8).collect();
        let application_output_cluster_list: Option<ApplicationClusterList> =
            Some(ApplicationClusterList(output_values));

        // when
        let simple_descriptor = SimpleDescriptor {
            endpoint,
            application_profile_identifier,
            application_device_identifier,
            application_device_version,
            application_input_cluster_count,
            application_input_cluster_list,
            application_output_cluster_count,
            application_output_cluster_list,
        };

        // then
        assert_eq!(simple_descriptor.endpoint, 42);
        assert_eq!(simple_descriptor.application_profile_identifier, 123);
        assert_eq!(simple_descriptor.application_device_identifier, 456);
        assert_eq!(simple_descriptor.application_device_version, 5);
        assert_eq!(simple_descriptor.application_input_cluster_count, 2);
        assert!(simple_descriptor.application_input_cluster_list.is_some());
        for i in 0..MAX_CLUSTER_COUNT {
            assert_eq!(
                simple_descriptor
                    .application_input_cluster_list
                    .as_ref()
                    .unwrap()
                    .0[i],
                (i + 1) as u8
            );
        }
        assert_eq!(simple_descriptor.application_output_cluster_count, 1);
        assert!(simple_descriptor.application_output_cluster_list.is_some());
        for i in 0..MAX_CLUSTER_COUNT {
            assert_eq!(
                simple_descriptor
                    .application_output_cluster_list
                    .as_ref()
                    .unwrap()
                    .0[i],
                (i + 2) as u8
            );
        }
    }

    #[test]
    fn creating_simple_descriptor_with_only_input_list_should_succeed() {
        // given
        let endpoint: u8 = 42;
        let application_profile_identifier: u16 = 123;
        let application_device_identifier: u16 = 456;
        let application_device_version: u8 = 5;

        let application_input_cluster_count: u8 = 2;
        let input_values: Vec<u8, MAX_CLUSTER_COUNT> =
            (0..MAX_CLUSTER_COUNT).map(|v| (v + 1) as u8).collect();
        let application_input_cluster_list: Option<ApplicationClusterList> =
            Some(ApplicationClusterList(input_values));

        let application_output_cluster_count: u8 = 0;
        let application_output_cluster_list: Option<ApplicationClusterList> = None;

        // when
        let simple_descriptor = SimpleDescriptor {
            endpoint,
            application_profile_identifier,
            application_device_identifier,
            application_device_version,
            application_input_cluster_count,
            application_input_cluster_list,
            application_output_cluster_count,
            application_output_cluster_list,
        };

        // then
        assert_eq!(simple_descriptor.endpoint, 42);
        assert_eq!(simple_descriptor.application_profile_identifier, 123);
        assert_eq!(simple_descriptor.application_device_identifier, 456);
        assert_eq!(simple_descriptor.application_device_version, 5);
        assert_eq!(simple_descriptor.application_input_cluster_count, 2);
        assert!(simple_descriptor
            .application_input_cluster_list
            .as_ref()
            .is_some());
        for i in 0..MAX_CLUSTER_COUNT {
            assert_eq!(
                simple_descriptor
                    .application_input_cluster_list
                    .as_ref()
                    .unwrap()
                    .0[i],
                (i + 1) as u8
            );
        }
        assert_eq!(simple_descriptor.application_output_cluster_count, 0);
        assert!(simple_descriptor
            .application_output_cluster_list
            .as_ref()
            .is_none());
    }

    #[test]
    fn creating_simple_descriptor_with_only_output_list_should_succeed() {
        // given
        let endpoint: u8 = 42;
        let application_profile_identifier: u16 = 123;
        let application_device_identifier: u16 = 456;
        let application_device_version: u8 = 5;
        let application_input_cluster_count: u8 = 0;
        let application_input_cluster_list: Option<ApplicationClusterList> = None;
        let application_output_cluster_count: u8 = 1;
        let output_values: Vec<u8, MAX_CLUSTER_COUNT> =
            (0..MAX_CLUSTER_COUNT).map(|v| (v + 2) as u8).collect();
        let application_output_cluster_list: Option<ApplicationClusterList> =
            Some(ApplicationClusterList(output_values));

        // when
        let simple_descriptor = SimpleDescriptor {
            endpoint,
            application_profile_identifier,
            application_device_identifier,
            application_device_version,
            application_input_cluster_count,
            application_input_cluster_list,
            application_output_cluster_count,
            application_output_cluster_list,
        };

        // then
        assert_eq!(simple_descriptor.endpoint, 42);
        assert_eq!(simple_descriptor.application_profile_identifier, 123);
        assert_eq!(simple_descriptor.application_device_identifier, 456);
        assert_eq!(simple_descriptor.application_device_version, 5);
        assert_eq!(simple_descriptor.application_input_cluster_count, 0);
        assert!(simple_descriptor
            .application_input_cluster_list
            .as_ref()
            .is_none());
        assert_eq!(simple_descriptor.application_output_cluster_count, 1);
        assert!(simple_descriptor
            .application_output_cluster_list
            .as_ref()
            .is_some());
        for i in 0..MAX_CLUSTER_COUNT {
            assert_eq!(
                simple_descriptor
                    .application_output_cluster_list
                    .as_ref()
                    .unwrap()
                    .0[i],
                (i + 2) as u8
            );
        }
    }
}
