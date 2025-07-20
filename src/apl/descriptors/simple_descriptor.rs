//! 2.3.2.5 Simple Descriptor
//!
//! The simple descriptor contains information specific to each endpoint
//! contained in this node. The simple descriptor is  mandatory for each
//! endpoint present in the node.

use crate::impl_byte;

type ApplicationClusterList<'a> = &'a [u8];

impl_byte! {
    pub struct SimpleDescriptor<'a> {
        endpoint: u8,
        application_profile_identifier: u16,
        application_device_identifier: u16,
        application_device_version: u8,
        application_input_cluster_count: u8,
        #[ctx = byte::ctx::Bytes::Len(application_input_cluster_count as usize)]
        application_input_cluster_list: ApplicationClusterList<'a>,
        application_output_cluster_count: u8,
        #[ctx = byte::ctx::Bytes::Len(application_output_cluster_count as usize)]
        application_output_cluster_list: ApplicationClusterList<'a>,
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;
    use byte::TryWrite;

    use super::*;

    const APPLICATION_INPUT_CLUSTER_COUNT: u8 = 15;
    const APPLICATION_OUTPUT_CLUSTER_COUNT: u8 = 9;

    #[test]
    fn creating_simple_descriptor_with_input_and_output_cluster_list_should_succeed() {
        // given
        // endpoint = 42
        // application_profile_identifier = 123
        // application_device_identifier = 456
        // application_device_version  = 5
        // application_input_cluster_count = 15
        // application_input_cluster_list = [0x01 - 0x0F]
        // application_output_cluster_count: u8 = 9
        // application_output_cluster_list = [0x02 - 0x0A]
        let bytes = [
            0x2A, 0x7B, 0x00, 0xC8, 0x01, 0x05, 0x0F, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x09, 0x02, 0x03, 0x04, 0x05, 0x06,
            0x07, 0x08, 0x09, 0x0A,
        ];

        // when
        let simple_descriptor = SimpleDescriptor::try_read(&bytes, ()).unwrap().0;

        // then
        assert_eq!(simple_descriptor.endpoint, 42);
        assert_eq!(simple_descriptor.application_profile_identifier, 123);
        assert_eq!(simple_descriptor.application_device_identifier, 456);
        assert_eq!(simple_descriptor.application_device_version, 5);
        assert_eq!(
            simple_descriptor.application_input_cluster_count,
            APPLICATION_INPUT_CLUSTER_COUNT
        );
        assert!(simple_descriptor.application_input_cluster_list.len() > 0);
        for i in 0..APPLICATION_INPUT_CLUSTER_COUNT as usize {
            assert_eq!(
                simple_descriptor.application_input_cluster_list[i],
                (i + 1) as u8
            );
        }
        assert_eq!(
            simple_descriptor.application_output_cluster_count,
            APPLICATION_OUTPUT_CLUSTER_COUNT
        );
        assert!(simple_descriptor.application_output_cluster_list.len() > 0);
        for i in 0..APPLICATION_OUTPUT_CLUSTER_COUNT as usize {
            assert_eq!(
                simple_descriptor.application_output_cluster_list[i],
                (i + 2) as u8
            );
        }
    }

    #[test]
    fn creating_simple_descriptor_with_only_input_list_should_succeed() {
        // given
        // endpoint = 42
        // application_profile_identifier = 123
        // application_device_identifier = 456
        // application_device_version  = 5
        // application_input_cluster_count = 15
        // application_input_cluster_list = [0x01 - 0x0F]
        // application_output_cluster_count: u8 = 0
        // application_output_cluster_list = []
        let bytes = [
            0x2A, 0x7B, 0x00, 0xC8, 0x01, 0x05, 0x0F, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x00,
        ];

        // when
        let simple_descriptor = SimpleDescriptor::try_read(&bytes, ()).unwrap().0;

        // then
        assert_eq!(simple_descriptor.endpoint, 42);
        assert_eq!(simple_descriptor.application_profile_identifier, 123);
        assert_eq!(simple_descriptor.application_device_identifier, 456);
        assert_eq!(simple_descriptor.application_device_version, 5);
        assert_eq!(
            simple_descriptor.application_input_cluster_count,
            APPLICATION_INPUT_CLUSTER_COUNT
        );
        assert!(simple_descriptor.application_input_cluster_list.len() > 0);
        for i in 0..APPLICATION_INPUT_CLUSTER_COUNT as usize {
            assert_eq!(
                simple_descriptor.application_input_cluster_list[i],
                (i + 1) as u8
            );
        }
        assert_eq!(simple_descriptor.application_output_cluster_count, 0);
        assert!(simple_descriptor.application_output_cluster_list.len() == 0);
    }

    #[test]
    fn creating_simple_descriptor_with_only_output_list_should_succeed() {
        // given
        // endpoint = 42
        // application_profile_identifier = 123
        // application_device_identifier = 456
        // application_device_version  = 5
        // application_input_cluster_count = 0
        // application_input_cluster_list = []
        // application_output_cluster_count: u8 = 9
        // application_output_cluster_list = [0x02 - 0x0A]
        let bytes = [
            0x2A, 0x7B, 0x00, 0xC8, 0x01, 0x05, 0x00, 0x09, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A,
        ];

        // when
        let simple_descriptor = SimpleDescriptor::try_read(&bytes, ()).unwrap().0;

        // then
        assert_eq!(simple_descriptor.endpoint, 42);
        assert_eq!(simple_descriptor.application_profile_identifier, 123);
        assert_eq!(simple_descriptor.application_device_identifier, 456);
        assert_eq!(simple_descriptor.application_device_version, 5);
        assert_eq!(simple_descriptor.application_input_cluster_count, 0);
        assert!(simple_descriptor.application_input_cluster_list.len() == 0);
        assert_eq!(
            simple_descriptor.application_output_cluster_count,
            APPLICATION_OUTPUT_CLUSTER_COUNT
        );
        assert!(simple_descriptor.application_output_cluster_list.len() > 0);
        for i in 0..APPLICATION_OUTPUT_CLUSTER_COUNT as usize {
            assert_eq!(
                simple_descriptor.application_output_cluster_list[i],
                (i + 2) as u8
            );
        }
    }

    #[test]
    fn writing_simple_descriptor_with_input_and_output_cluster_list_should_succeed() {
        // given
        // endpoint = 42
        // application_profile_identifier = 123
        // application_device_identifier = 456
        // application_device_version  = 5
        // application_input_cluster_count = 15
        // application_input_cluster_list = [0x01 - 0x0F]
        // application_output_cluster_count: u8 = 9
        // application_output_cluster_list = [0x02 - 0x0A]
        let application_input_cluster_list = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F,
        ];
        let application_output_cluster_list =
            [0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];

        let simple_descriptor = SimpleDescriptor {
            endpoint: 42,
            application_profile_identifier: 123,
            application_device_identifier: 456,
            application_device_version: 5,
            application_input_cluster_count: 15,
            application_input_cluster_list: &application_input_cluster_list,
            application_output_cluster_count: 9,
            application_output_cluster_list: &application_output_cluster_list,
        };
        let mut bytes: [u8; 32] = [0; 32];

        // when
        let bytes_written = simple_descriptor.try_write(&mut bytes, ()).unwrap();

        // then
        let expected_bytes = [
            0x2A, 0x7B, 0x00, 0xC8, 0x01, 0x05, 0x0F, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x09, 0x02, 0x03, 0x04, 0x05, 0x06,
            0x07, 0x08, 0x09, 0x0A,
        ];

        assert_eq!(bytes_written, 32);
        assert_eq!(expected_bytes, bytes);
    }

    #[test]
    fn writting_simple_descriptor_with_only_input_list_should_succeed() {
        // given
        // endpoint = 42
        // application_profile_identifier = 123
        // application_device_identifier = 456
        // application_device_version  = 5
        // application_input_cluster_count = 15
        // application_input_cluster_list = [0x01 - 0x0F]
        // application_output_cluster_count: u8 = 0
        // application_output_cluster_list = []
        let application_input_cluster_list = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F,
        ];
        let simple_descriptor = SimpleDescriptor {
            endpoint: 42,
            application_profile_identifier: 123,
            application_device_identifier: 456,
            application_device_version: 5,
            application_input_cluster_count: 15,
            application_input_cluster_list: &application_input_cluster_list,
            application_output_cluster_count: 0,
            application_output_cluster_list: &[],
        };
        let mut bytes: [u8; 23] = [0; 23];

        //when
        let bytes_written = simple_descriptor.try_write(&mut bytes, ()).unwrap();

        //then
        let expected_bytes = [
            0x2A, 0x7B, 0x00, 0xC8, 0x01, 0x05, 0x0F, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x00,
        ];
        assert_eq!(bytes_written, 23);
        assert_eq!(expected_bytes, bytes);
    }

    #[test]
    fn writting_simple_descriptor_with_only_output_list_should_succeed() {
        // given
        // endpoint = 42
        // application_profile_identifier = 123
        // application_device_identifier = 456
        // application_device_version  = 5
        // application_input_cluster_count = 0
        // application_input_cluster_list = []
        // application_output_cluster_count: u8 = 9
        // application_output_cluster_list = [0x02 - 0x0A]
        let application_output_cluster_list =
            [0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];

        let simple_descriptor = SimpleDescriptor {
            endpoint: 42,
            application_profile_identifier: 123,
            application_device_identifier: 456,
            application_device_version: 5,
            application_input_cluster_count: 0,
            application_input_cluster_list: &[],
            application_output_cluster_count: 9,
            application_output_cluster_list: &application_output_cluster_list,
        };
        let mut bytes: [u8; 17] = [0; 17];

        // when
        let bytes_written = simple_descriptor.try_write(&mut bytes, ()).unwrap();

        // then
        let expected_bytes = [
            0x2A, 0x7B, 0x00, 0xC8, 0x01, 0x05, 0x00, 0x09, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A,
        ];
        assert_eq!(bytes_written, 17);
        assert_eq!(expected_bytes, bytes);
    }
}
