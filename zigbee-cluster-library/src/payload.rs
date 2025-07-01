use byte::{BytesExt, TryRead, TryWrite};
use heapless::Vec;

use crate::{
    frame::{AttributeReport, GeneralCommand},
    header::{command_identifier::CommandIdentifier, frame_control::FrameType, ZclHeader},
};

#[derive(Debug)]
pub enum ZclFramePayload<'a> {
    GeneralCommand(GeneralCommand<'a>),
    ClusterSpecificCommand(&'a [u8]),
    Reserved,
}

impl<'a> TryRead<'a, &ZclHeader> for ZclFramePayload<'a> {
    fn try_read(bytes: &'a [u8], header: &ZclHeader) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let payload = match header.frame_control.frame_type() {
            FrameType::GlobalCommand => {
                let cmd = match header.command_identifier {
                    // ReadAttributes => todo!(),
                    // ReadAttributesResponse => todo!(),
                    // WriteAttributes => todo!(),
                    // WriteAttributesUndivided => todo!(),
                    // WriteAttributesResponse => todo!(),
                    // WriteAttributesNoResponse => todo!(),
                    // ConfigureReporting => todo!(),
                    // ConfigureReportingResponse => todo!(),
                    // ReadReportingConfiguration => todo!(),
                    // ReadReportingConfigurationResponse => todo!(),
                    CommandIdentifier::ReportAttributes => {
                        let mut attribute_reports: Vec<AttributeReport<'_>, 16> = Vec::new();
                        while let Ok(attribute_report) = bytes.read_with(offset, ()) {
                            attribute_reports.push(attribute_report).unwrap();
                        }
                        GeneralCommand::ReportAttributesCommand(attribute_reports)
                    }
                    // DefaultResponse => todo!(),
                    // DiscoverAttributes => todo!(),
                    // DiscoverAttributesResponse => todo!(),
                    // ReadAttributesStructured => todo!(),
                    // WriteAttributesStructured => todo!(),
                    // WriteAttributesStructuredResponse => todo!(),
                    // DiscoverCommandsReceived => todo!(),
                    // DiscoverCommandsReceivedResponse => todo!(),
                    // DiscoverCommandsGenerated => todo!(),
                    // DiscoverCommandsGeneratedResponse => todo!(),
                    // DiscoverAttributesExtended => todo!(),
                    // DiscoverAttributesExtendedResponse => todo!(),
                    // Reserved => todo!(),
                    _ => todo!(),
                };
                ZclFramePayload::GeneralCommand(cmd)
            }
            FrameType::ClusterCommand => todo!(),
            FrameType::Reserved => todo!(),
        };

        Ok((payload, *offset))
    }
}
impl TryWrite<&ZclHeader> for ZclFramePayload<'_> {
    fn try_write(self, _bytes: &mut [u8], _header: &ZclHeader) -> Result<usize, ::byte::Error> {
        unimplemented!()
    }
}
