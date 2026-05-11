use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use heapless::Vec;

use crate::frame::GeneralCommand;
use crate::frame::Status;
use crate::frame::WriteAttributeStatus;
use crate::header::ZclHeader;
use crate::header::command_identifier::CommandIdentifier;
use crate::header::frame_control::FrameType;

#[derive(Debug, PartialEq)]
pub enum ZclFramePayload<'a> {
    GeneralCommand(GeneralCommand<'a>),
    ClusterSpecificCommand(&'a [u8]),
    Reserved,
}

fn read_records<'a, T, const N: usize>(
    bytes: &'a [u8],
    offset: &mut usize,
) -> byte::Result<Vec<T, N>>
where
    T: TryRead<'a, ()>,
{
    let mut records = Vec::new();
    while *offset < bytes.len() {
        let record = bytes.read_with(offset, ())?;
        records
            .push(record)
            .map_err(|_| bad_input!("ZCL command record list exceeds capacity"))?;
    }
    Ok(records)
}

/// Normalizes a WriteAttributesResponse for serialization:
/// - If any failures present: emit only failure records (spec: no SUCCESS mixed in).
/// - Otherwise (all success or empty): emit a single SUCCESS record with no attribute id.
fn write_normalized_response<const N: usize>(
    bytes: &mut [u8],
    offset: &mut usize,
    records: Vec<WriteAttributeStatus, N>,
) -> byte::Result<()> {
    let has_failures = records.iter().any(|r| r.status != Status::Success);
    if has_failures {
        for record in records {
            if record.status != Status::Success {
                bytes.write_with(offset, record, ())?;
            }
        }
    } else {
        bytes.write_with(
            offset,
            WriteAttributeStatus {
                status: Status::Success,
                attribute_id: None,
            },
            (),
        )?;
    }
    Ok(())
}

fn write_records<T, const N: usize>(
    bytes: &mut [u8],
    offset: &mut usize,
    records: Vec<T, N>,
) -> byte::Result<()>
where
    T: TryWrite<()>,
{
    for record in records {
        bytes.write_with(offset, record, ())?;
    }
    Ok(())
}

impl<'a> TryRead<'a, &ZclHeader> for ZclFramePayload<'a> {
    fn try_read(bytes: &'a [u8], header: &ZclHeader) -> Result<(Self, usize), ::byte::Error> {
        let offset = &mut 0;
        let payload = match header.frame_control.frame_type() {
            FrameType::GlobalCommand => {
                let cmd = match header.command_identifier {
                    CommandIdentifier::ReadAttributes => {
                        GeneralCommand::ReadAttributes(read_records(bytes, offset)?)
                    }
                    CommandIdentifier::ReadAttributesResponse => {
                        GeneralCommand::ReadAttributesResponse(read_records(bytes, offset)?)
                    }
                    CommandIdentifier::WriteAttributes => {
                        GeneralCommand::WriteAttributes(read_records(bytes, offset)?)
                    }
                    CommandIdentifier::WriteAttributesResponse => {
                        GeneralCommand::WriteAttributesResponse(read_records(bytes, offset)?)
                    }
                    CommandIdentifier::ReportAttributes => {
                        GeneralCommand::ReportAttributes(read_records(bytes, offset)?)
                    }
                    CommandIdentifier::DefaultResponse => {
                        GeneralCommand::DefaultResponse(bytes.read_with(offset, ())?)
                    }
                    _ => {
                        return Err(bad_input!("unsupported ZCL global command"));
                    }
                };
                ZclFramePayload::GeneralCommand(cmd)
            }
            FrameType::ClusterCommand => {
                *offset = bytes.len();
                ZclFramePayload::ClusterSpecificCommand(bytes)
            }
            FrameType::Reserved => {
                return Err(bad_input!("reserved ZCL frame type"));
            }
        };

        Ok((payload, *offset))
    }
}

impl TryWrite<()> for ZclFramePayload<'_> {
    fn try_write(self, bytes: &mut [u8], _: ()) -> Result<usize, ::byte::Error> {
        let offset = &mut 0;
        match self {
            Self::GeneralCommand(command) => match command {
                GeneralCommand::ReadAttributes(attrs) => write_records(bytes, offset, attrs)?,
                GeneralCommand::ReadAttributesResponse(attrs) => {
                    write_records(bytes, offset, attrs)?
                }
                GeneralCommand::WriteAttributes(attrs) => write_records(bytes, offset, attrs)?,
                GeneralCommand::WriteAttributesResponse(attrs) => {
                    write_normalized_response(bytes, offset, attrs)?
                }
                GeneralCommand::ReportAttributes(attrs) => write_records(bytes, offset, attrs)?,
                GeneralCommand::DefaultResponse(response) => {
                    bytes.write_with(offset, response, ())?;
                }
            },
            Self::ClusterSpecificCommand(payload) => {
                let end = payload.len();
                bytes
                    .get_mut(..end)
                    .ok_or(byte::Error::Incomplete)?
                    .copy_from_slice(payload);
                *offset = end;
            }
            Self::Reserved => {
                return Err(bad_input!("reserved ZCL payload"));
            }
        }
        Ok(*offset)
    }
}
