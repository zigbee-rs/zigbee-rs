//! Color Control Cluster
//!
//! See ZCL Section 5.2.

use core::sync::atomic::AtomicU8;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::Ordering;

use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::reporting::AttributeReporting;
use crate::server::ClusterCommand;
use crate::server::ClusterServer;
use crate::server::CommandOutcome;
use crate::types::bitmaps::Bitmap16;
use crate::types::bitmaps::ZclBitmap16;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::enums::Enum8;
use crate::types::enums::ZclEnum8;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Uint8;
use crate::types::integers::Uint16;

pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0300), "ColorControl");
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// Color-temperature mode, as advertised in `ColorCapabilities`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ColorCapabilities(u16);

impl ZclBitmap16 for ColorCapabilities {
    fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    fn into_bits(self) -> u16 {
        self.0
    }
}

/// Color mode enumeration (ZCL 5.2.2.2.1).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    HueSaturation = 0,
    Xy = 1,
    ColorTemperature = 2,
}

impl ZclEnum8 for ColorMode {
    fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::HueSaturation),
            1 => Some(Self::Xy),
            2 => Some(Self::ColorTemperature),
            _ => None,
        }
    }

    fn into_raw(self) -> u8 {
        self as u8
    }
}

pub const CURRENT_HUE: Attribute<Uint8> = CLUSTER.attribute(AttributeId(0x0000), "CurrentHue");
pub const CURRENT_SATURATION: Attribute<Uint8> =
    CLUSTER.attribute(AttributeId(0x0001), "CurrentSaturation");
pub const REMAINING_TIME: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x0002), "RemainingTime");
pub const COLOR_MODE: Attribute<Enum8<ColorMode>> =
    CLUSTER.attribute(AttributeId(0x0008), "ColorMode");
pub const COLOR_TEMPERATURE_MIREDS: Attribute<Uint16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0007), "ColorTemperatureMireds");
pub const COLOR_CAPABILITIES: Attribute<Bitmap16<ColorCapabilities>> =
    CLUSTER.attribute(AttributeId(0x400a), "ColorCapabilities");
pub const COLOR_TEMPERATURE_PHYSICAL_MIN_MIREDS: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x400b), "ColorTempPhysicalMinMireds");
pub const COLOR_TEMPERATURE_PHYSICAL_MAX_MIREDS: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x400c), "ColorTempPhysicalMaxMireds");

const ATTRIBUTES: &[AttrInfo] = &[
    CURRENT_HUE.attr_info(),
    CURRENT_SATURATION.attr_info(),
    REMAINING_TIME.attr_info(),
    COLOR_TEMPERATURE_MIREDS.attr_info(),
    COLOR_MODE.attr_info(),
    COLOR_CAPABILITIES.attr_info(),
    COLOR_TEMPERATURE_PHYSICAL_MIN_MIREDS.attr_info(),
    COLOR_TEMPERATURE_PHYSICAL_MAX_MIREDS.attr_info(),
];

pub mod attribute {
    pub const COLOR_TEMPERATURE_MIREDS: u16 = super::COLOR_TEMPERATURE_MIREDS.id().0;
}

pub mod command {
    pub const MOVE_TO_COLOR_TEMPERATURE: u8 = 0x0a;
}

pub const PHYSICAL_MIN_MIREDS: u16 = 153;
pub const PHYSICAL_MAX_MIREDS: u16 = 454;
const COLOR_TEMPERATURE_CAPABILITY: u16 = 0x0010;

/// Single-light Color Control server for color-temperature commands.
pub struct ColorControlServer<'a> {
    color_temperature_mireds: AtomicU16,
    color_mode: AtomicU8,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> ColorControlServer<'a> {
    pub const fn new(initial_mireds: u16) -> Self {
        Self {
            color_temperature_mireds: AtomicU16::new(clamp_mireds(initial_mireds)),
            color_mode: AtomicU8::new(ColorMode::ColorTemperature as u8),
            reporting: None,
        }
    }

    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    pub fn color_temperature_mireds(&self) -> u16 {
        self.color_temperature_mireds.load(Ordering::Relaxed)
    }

    pub fn set_color_temperature_mireds(&self, mireds: u16) {
        self.color_temperature_mireds
            .store(clamp_mireds(mireds), Ordering::Relaxed);
        self.color_mode
            .store(ColorMode::ColorTemperature as u8, Ordering::Relaxed);
    }
}

const fn clamp_mireds(mireds: u16) -> u16 {
    if mireds < PHYSICAL_MIN_MIREDS {
        PHYSICAL_MIN_MIREDS
    } else if mireds > PHYSICAL_MAX_MIREDS {
        PHYSICAL_MAX_MIREDS
    } else {
        mireds
    }
}

impl ClusterServer for ColorControlServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        ATTRIBUTES
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        let encoded = match id.0 {
            attribute::COLOR_TEMPERATURE_MIREDS => COLOR_TEMPERATURE_MIREDS.encode(
                Uint16(self.color_temperature_mireds()),
                out,
                offset,
            ),
            0x0000 => CURRENT_HUE.encode(Uint8(0), out, offset),
            0x0001 => CURRENT_SATURATION.encode(Uint8(0), out, offset),
            0x0002 => REMAINING_TIME.encode(Uint16(0), out, offset),
            0x0008 => COLOR_MODE.encode(
                ColorMode::from_raw(self.color_mode.load(Ordering::Relaxed))
                    .unwrap_or(ColorMode::ColorTemperature),
                out,
                offset,
            ),
            0x400a => COLOR_CAPABILITIES.encode(
                ColorCapabilities(COLOR_TEMPERATURE_CAPABILITY),
                out,
                offset,
            ),
            0x400b => COLOR_TEMPERATURE_PHYSICAL_MIN_MIREDS.encode(
                Uint16(PHYSICAL_MIN_MIREDS),
                out,
                offset,
            ),
            0x400c => COLOR_TEMPERATURE_PHYSICAL_MAX_MIREDS.encode(
                Uint16(PHYSICAL_MAX_MIREDS),
                out,
                offset,
            ),
            _ => return Status::UnsupportedAttribute,
        };
        encoded.map_or_else(|_| Status::InsufficientSpace, |_| Status::Success)
    }

    fn reporting(&self) -> Option<&dyn AttributeReporting> {
        self.reporting
    }

    fn command(&self, command: ClusterCommand<'_>, _out: &mut [u8]) -> CommandOutcome {
        if command.id.0 != command::MOVE_TO_COLOR_TEMPERATURE {
            return CommandOutcome::Status(Status::UnsupCommand);
        }
        let Some(data) = command.data.get(..4) else {
            return CommandOutcome::Status(Status::MalformedCommand);
        };
        let mireds = u16::from_le_bytes([data[0], data[1]]);
        if !(PHYSICAL_MIN_MIREDS..=PHYSICAL_MAX_MIREDS).contains(&mireds) {
            return CommandOutcome::Status(Status::InvalidField);
        }
        self.set_color_temperature_mireds(mireds);
        CommandOutcome::Status(Status::Success)
    }
}

impl ClusterRequestHandler for ColorControlServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::CommandId;

    fn run(server: &ColorControlServer<'_>, id: u8, data: &[u8]) -> CommandOutcome {
        let mut out = [0u8; 16];
        server.command(
            ClusterCommand {
                id: CommandId(id),
                data,
                sequence_number: 1,
            },
            &mut out,
        )
    }

    #[test]
    fn move_to_temperature_updates_the_mired_value() {
        let server = ColorControlServer::new(370);
        assert_eq!(
            run(
                &server,
                command::MOVE_TO_COLOR_TEMPERATURE,
                &[0x72, 0x01, 0, 0]
            ),
            CommandOutcome::Status(Status::Success)
        );
        assert_eq!(server.color_temperature_mireds(), 370);
    }

    #[test]
    fn move_to_temperature_rejects_out_of_range_values() {
        let server = ColorControlServer::new(370);
        assert_eq!(
            run(&server, command::MOVE_TO_COLOR_TEMPERATURE, &[0, 0, 0, 0]),
            CommandOutcome::Status(Status::InvalidField)
        );
        assert_eq!(server.color_temperature_mireds(), 370);
    }

    #[test]
    fn color_temperature_encodes_as_uint16() {
        let server = ColorControlServer::new(370);
        let mut out = [0u8; 8];
        let mut offset = 0;
        assert_eq!(
            server.encode_value(COLOR_TEMPERATURE_MIREDS.id(), &mut out, &mut offset),
            Status::Success
        );
        assert_eq!(&out[..offset], &[0x21, 0x72, 0x01]);
    }
}
