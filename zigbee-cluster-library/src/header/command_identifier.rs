//! Command Identifier.
use byte::TryRead;
use byte::TryWrite;

/// Command Identifier.
///
/// See Section 2.4.1.4 (Values can be found in Table 2-3)
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum CommandIdentifier {
    ReadAttributes = 0x00,
    ReadAttributesResponse = 0x01,
    WriteAttributes = 0x02,
    WriteAttributesUndivided = 0x03,
    WriteAttributesResponse = 0x04,
    WriteAttributesNoResponse = 0x05,
    ConfigureReporting = 0x06,
    ConfigureReportingResponse = 0x07,
    ReadReportingConfiguration = 0x08,
    ReadReportingConfigurationResponse = 0x09,
    ReportAttributes = 0x0a,
    DefaultResponse = 0x0b,
    DiscoverAttributes = 0x0c,
    DiscoverAttributesResponse = 0x0d,
    ReadAttributesStructured = 0x0e,
    WriteAttributesStructured = 0x0f,
    WriteAttributesStructuredResponse = 0x10,
    DiscoverCommandsReceived = 0x11,
    DiscoverCommandsReceivedResponse = 0x12,
    DiscoverCommandsGenerated = 0x13,
    DiscoverCommandsGeneratedResponse = 0x14,
    DiscoverAttributesExtended = 0x15,
    DiscoverAttributesExtendedResponse = 0x16,
    Reserved = 0xff,
}

impl CommandIdentifier {
    fn from_bits(b: u8) -> Self {
        match b {
            0x00 => Self::ReadAttributes,
            0x01 => Self::ReadAttributesResponse,
            0x02 => Self::WriteAttributes,
            0x03 => Self::WriteAttributesUndivided,
            0x04 => Self::WriteAttributesResponse,
            0x05 => Self::WriteAttributesNoResponse,
            0x06 => Self::ConfigureReporting,
            0x07 => Self::ConfigureReportingResponse,
            0x08 => Self::ReadReportingConfiguration,
            0x09 => Self::ReadReportingConfigurationResponse,
            0x0a => Self::ReportAttributes,
            0x0b => Self::DefaultResponse,
            0x0c => Self::DiscoverAttributes,
            0x0d => Self::DiscoverAttributesResponse,
            0x0e => Self::ReadAttributesStructured,
            0x0f => Self::WriteAttributesStructured,
            0x10 => Self::WriteAttributesStructuredResponse,
            0x11 => Self::DiscoverCommandsReceived,
            0x12 => Self::DiscoverCommandsReceivedResponse,
            0x13 => Self::DiscoverCommandsGenerated,
            0x14 => Self::DiscoverCommandsGeneratedResponse,
            0x15 => Self::DiscoverAttributesExtended,
            0x16 => Self::DiscoverAttributesExtendedResponse,
            _ => Self::Reserved,
        }
    }
}

impl TryRead<'_, byte::ctx::Endian> for CommandIdentifier {
    fn try_read(bytes: &[u8], ctx: byte::ctx::Endian) -> byte::Result<(Self, usize)> {
        let (value, size) = u8::try_read(bytes, ctx)?;
        Ok((Self::from_bits(value), size))
    }
}

impl TryWrite<byte::ctx::Endian> for CommandIdentifier {
    fn try_write(self, bytes: &mut [u8], ctx: byte::ctx::Endian) -> byte::Result<usize> {
        (self as u8).try_write(bytes, ctx)
    }
}

#[cfg(test)]
mod tests {
    use byte::TryRead;

    use super::*;
    use crate::test_branches_command_identifier;

    test_branches_command_identifier! {
        cmd_ident_read_attributes: &[0x00] => CommandIdentifier::ReadAttributes,
        cmd_ident_read_attributes_response: &[0x01] => CommandIdentifier::ReadAttributesResponse,
        cmd_ident_write_attributes: &[0x02] => CommandIdentifier::WriteAttributes,
        cmd_ident_write_attributes_undivided: &[0x03] => CommandIdentifier::WriteAttributesUndivided,
        cmd_ident_write_attributes_response: &[0x04] => CommandIdentifier::WriteAttributesResponse,
        cmd_ident_write_attributes_no_response: &[0x05] => CommandIdentifier::WriteAttributesNoResponse,
        cmd_ident_configure_reporting: &[0x06] => CommandIdentifier::ConfigureReporting,
        cmd_ident_configure_reporting_response: &[0x07] => CommandIdentifier::ConfigureReportingResponse,
        cmd_ident_read_reporting_configuration: &[0x08] => CommandIdentifier::ReadReportingConfiguration,
        cmd_ident_read_reporting_configuration_response: &[0x09] => CommandIdentifier::ReadReportingConfigurationResponse,
        cmd_ident_report_attributes: &[0x0a] => CommandIdentifier::ReportAttributes,
        cmd_ident_default_response: &[0x0b] => CommandIdentifier::DefaultResponse,
        cmd_ident_discover_attributes: &[0x0c] => CommandIdentifier::DiscoverAttributes,
        cmd_ident_discover_attributes_response: &[0x0d] => CommandIdentifier::DiscoverAttributesResponse,
        cmd_ident_read_attributes_structured: &[0x0e] => CommandIdentifier::ReadAttributesStructured,
        cmd_ident_write_attributes_structured: &[0x0f] => CommandIdentifier::WriteAttributesStructured,
        cmd_ident_write_attributes_structured_response: &[0x10] => CommandIdentifier::WriteAttributesStructuredResponse,
        cmd_ident_discover_commands_received: &[0x11] => CommandIdentifier::DiscoverCommandsReceived,
        cmd_ident_discovercommandsreceivedresponse: &[0x12] => CommandIdentifier::DiscoverCommandsReceivedResponse,
        cmd_ident_discovercommandsgenerated: &[0x13] => CommandIdentifier::DiscoverCommandsGenerated,
        cmd_ident_discovercommandsgeneratedresponse: &[0x14] => CommandIdentifier::DiscoverCommandsGeneratedResponse,
        cmd_ident_discoverattributesextended: &[0x15] => CommandIdentifier::DiscoverAttributesExtended,
        cmd_ident_discoverattributesextendedresponse: &[0x16] => CommandIdentifier::DiscoverAttributesExtendedResponse,
    }
}

#[macro_export]
macro_rules! test_branches_command_identifier {
    ($($name:ident: $input:expr => $want:expr,)+) => {
        $(
            #[test]
            fn $name() {
                // when
                let (command_identifier, _) = CommandIdentifier::try_read($input, byte::LE)
                    .expect("Could not read CommandIdentifier in test");

                // then
                assert_eq!(command_identifier, $want);
            }
        )+
    };
}
