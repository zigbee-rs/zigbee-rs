//! Color Control Cluster
//!
//! See ZCL Section 5.2.

use core::sync::atomic::AtomicU16;
use core::sync::atomic::AtomicU32;
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

pub const CURRENT_HUE: Attribute<Uint8, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "CurrentHue");
pub const CURRENT_SATURATION: Attribute<Uint8, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0001), "CurrentSaturation");
pub const REMAINING_TIME: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x0002), "RemainingTime");
pub const CURRENT_X: Attribute<Uint16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0003), "CurrentX");
pub const CURRENT_Y: Attribute<Uint16, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0004), "CurrentY");
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
    CURRENT_X.attr_info(),
    CURRENT_Y.attr_info(),
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
    pub const MOVE_TO_HUE: u8 = 0x00;
    pub const MOVE_TO_SATURATION: u8 = 0x03;
    pub const MOVE_TO_HUE_AND_SATURATION: u8 = 0x06;
    pub const MOVE_TO_COLOR: u8 = 0x07;
    pub const MOVE_TO_COLOR_TEMPERATURE: u8 = 0x0a;
}

pub const PHYSICAL_MIN_MIREDS: u16 = 153;
pub const PHYSICAL_MAX_MIREDS: u16 = 454;
/// Highest `CurrentX` / `CurrentY` value, and so the highest x or y a Move to
/// Color command may carry (ZCL Table 5.3).
const MAX_XY: u16 = 0xfeff;
const COLOR_CAPABILITIES_SUPPORTED: u16 = 0x0019;

/// Single-light Color Control server for hue, saturation and color temperature.
pub struct ColorControlServer<'a> {
    color_temperature_mireds: AtomicU16,
    color: AtomicU32,
    xy: AtomicU32,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> ColorControlServer<'a> {
    pub const fn new(initial_mireds: u16) -> Self {
        Self {
            color_temperature_mireds: AtomicU16::new(clamp_mireds(initial_mireds)),
            color: AtomicU32::new((ColorMode::ColorTemperature as u32) << 16),
            xy: AtomicU32::new(0x5439_500f),
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

    pub fn color(&self) -> (ColorMode, u8, u8) {
        let value = self.color.load(Ordering::Relaxed);
        let [hue, saturation, mode, _] = value.to_le_bytes();
        let mode = ColorMode::from_raw(mode).unwrap_or(ColorMode::ColorTemperature);
        (mode, hue, saturation)
    }

    pub fn set_hue_and_saturation(&self, hue: u8, saturation: u8) {
        let value = u32::from(hue.min(254))
            | (u32::from(saturation.min(254)) << 8)
            | ((ColorMode::HueSaturation as u32) << 16);
        self.color.store(value, Ordering::Relaxed);
    }

    pub fn xy(&self) -> (u16, u16) {
        let value = self.xy.load(Ordering::Relaxed);
        let [x_low, x_high, y_low, y_high] = value.to_le_bytes();
        (
            u16::from_le_bytes([x_low, x_high]),
            u16::from_le_bytes([y_low, y_high]),
        )
    }

    pub fn set_xy(&self, x: u16, y: u16) {
        self.xy
            .store(u32::from(x) | (u32::from(y) << 16), Ordering::Relaxed);
        let _ = self
            .color
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some((value & 0xffff) | ((ColorMode::Xy as u32) << 16))
            });
    }

    pub fn set_color_temperature_mireds(&self, mireds: u16) {
        self.color_temperature_mireds
            .store(clamp_mireds(mireds), Ordering::Relaxed);
        let _ = self
            .color
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some((value & 0xffff) | ((ColorMode::ColorTemperature as u32) << 16))
            });
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
            0x0000 => CURRENT_HUE.encode(Uint8(self.color().1), out, offset),
            0x0001 => CURRENT_SATURATION.encode(Uint8(self.color().2), out, offset),
            0x0002 => REMAINING_TIME.encode(Uint16(0), out, offset),
            0x0003 => CURRENT_X.encode(Uint16(self.xy().0), out, offset),
            0x0004 => CURRENT_Y.encode(Uint16(self.xy().1), out, offset),
            0x0008 => COLOR_MODE.encode(self.color().0, out, offset),
            0x400a => COLOR_CAPABILITIES.encode(
                ColorCapabilities(COLOR_CAPABILITIES_SUPPORTED),
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
        match command.id.0 {
            command::MOVE_TO_HUE => {
                let Some(data) = command.data.get(..4) else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                if data[0] == 255 || data[1] > 3 {
                    return CommandOutcome::Status(Status::InvalidField);
                }
                self.set_hue_and_saturation(data[0], self.color().2);
            }
            command::MOVE_TO_SATURATION => {
                let Some(data) = command.data.get(..3) else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                if data[0] == 255 {
                    return CommandOutcome::Status(Status::InvalidField);
                }
                self.set_hue_and_saturation(self.color().1, data[0]);
            }
            command::MOVE_TO_HUE_AND_SATURATION => {
                let Some(data) = command.data.get(..4) else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                if data[0] == 255 || data[1] == 255 {
                    return CommandOutcome::Status(Status::InvalidField);
                }
                self.set_hue_and_saturation(data[0], data[1]);
            }
            command::MOVE_TO_COLOR => {
                let Some(data) = command.data.get(..6) else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                let x = u16::from_le_bytes([data[0], data[1]]);
                let y = u16::from_le_bytes([data[2], data[3]]);
                if x > MAX_XY || y > MAX_XY {
                    return CommandOutcome::Status(Status::InvalidField);
                }
                self.set_xy(x, y);
            }
            command::MOVE_TO_COLOR_TEMPERATURE => {
                let Some(data) = command.data.get(..4) else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                // a value outside the physical range is clamped to it, not
                // rejected
                self.set_color_temperature_mireds(u16::from_le_bytes([data[0], data[1]]));
            }
            _ => return CommandOutcome::Status(Status::UnsupCommand),
        }
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
    fn move_to_temperature_clamps_out_of_range_values() {
        let server = ColorControlServer::new(370);
        // below the physical minimum, zero, and above the physical maximum
        for (mireds, clamped) in [
            (100u16, PHYSICAL_MIN_MIREDS),
            (0, PHYSICAL_MIN_MIREDS),
            (1000, PHYSICAL_MAX_MIREDS),
            (u16::MAX, PHYSICAL_MAX_MIREDS),
        ] {
            let [low, high] = mireds.to_le_bytes();
            assert_eq!(
                run(
                    &server,
                    command::MOVE_TO_COLOR_TEMPERATURE,
                    &[low, high, 0, 0]
                ),
                CommandOutcome::Status(Status::Success)
            );
            assert_eq!(server.color_temperature_mireds(), clamped);
            assert!(server.color().0 == ColorMode::ColorTemperature);
        }
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

    #[test]
    fn hue_saturation_commands_switch_modes_and_preserve_values() {
        let server = ColorControlServer::new(370);
        assert_eq!(
            run(
                &server,
                command::MOVE_TO_HUE_AND_SATURATION,
                &[42, 180, 0, 0]
            ),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.color() == (ColorMode::HueSaturation, 42, 180));
        assert_eq!(
            run(&server, command::MOVE_TO_HUE, &[90, 0, 0, 0]),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.color() == (ColorMode::HueSaturation, 90, 180));
        assert_eq!(
            run(
                &server,
                command::MOVE_TO_COLOR_TEMPERATURE,
                &[0x72, 0x01, 0, 0]
            ),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.color() == (ColorMode::ColorTemperature, 90, 180));
        assert_eq!(
            run(&server, command::MOVE_TO_SATURATION, &[80, 0, 0]),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.color() == (ColorMode::HueSaturation, 90, 80));
    }

    #[test]
    fn malformed_color_commands_do_not_change_state() {
        let server = ColorControlServer::new(370);
        assert_eq!(
            run(&server, command::MOVE_TO_HUE_AND_SATURATION, &[1, 2]),
            CommandOutcome::Status(Status::MalformedCommand)
        );
        assert_eq!(
            run(&server, command::MOVE_TO_SATURATION, &[255, 0, 0]),
            CommandOutcome::Status(Status::InvalidField)
        );
        assert!(server.color() == (ColorMode::ColorTemperature, 0, 0));
    }

    #[test]
    fn hue_saturation_command_is_dispatched_from_a_zigbee_frame() {
        let server = ColorControlServer::new(370);
        let mut out = [0u8; 32];
        let asdu = [0x01, 0x42, 0x06, 42, 180, 0, 0];
        let request = ClusterRequest {
            profile_id: 0x0104,
            cluster_id: CLUSTER_ID,
            src_endpoint: 1,
            dst_endpoint: 1,
            unicast: true,
            asdu: &asdu,
        };
        let reply = server.handle(&request, &mut out).expect("command response");
        assert_eq!(&out[..reply.len], &[0x18, 0x42, 0x0b, 0x06, 0x00]);
        assert!(server.color() == (ColorMode::HueSaturation, 42, 180));

        let mut offset = 0;
        assert_eq!(
            server.encode_value(CURRENT_HUE.id(), &mut out, &mut offset),
            Status::Success
        );
        assert_eq!(&out[..offset], &[0x20, 42]);
        offset = 0;
        assert_eq!(
            server.encode_value(COLOR_CAPABILITIES.id(), &mut out, &mut offset),
            Status::Success
        );
        assert_eq!(&out[..offset], &[0x19, 0x19, 0x00]);
    }

    #[test]
    fn xy_command_updates_attributes_and_switches_modes() {
        let server = ColorControlServer::new(370);
        assert_eq!(
            run(
                &server,
                command::MOVE_TO_COLOR,
                &[0x34, 0x12, 0x78, 0x56, 0, 0]
            ),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.color().0 == ColorMode::Xy);
        assert_eq!(server.xy(), (0x1234, 0x5678));
        let mut out = [0; 8];
        let mut offset = 0;
        assert_eq!(
            server.encode_value(CURRENT_X.id(), &mut out, &mut offset),
            Status::Success
        );
        assert_eq!(&out[..offset], &[0x21, 0x34, 0x12]);
        assert_eq!(
            run(&server, command::MOVE_TO_COLOR, &[0, 0]),
            CommandOutcome::Status(Status::MalformedCommand)
        );
        assert_eq!(
            run(&server, command::MOVE_TO_COLOR, &[255, 255, 0, 0, 0, 0]),
            CommandOutcome::Status(Status::InvalidField)
        );
        assert_eq!(server.xy(), (0x1234, 0x5678));
        // 0xfeff is the highest valid x or y (ZCL Table 5.3); 0xff00 is not
        assert_eq!(
            run(&server, command::MOVE_TO_COLOR, &[0x00, 0xff, 0, 0, 0, 0]),
            CommandOutcome::Status(Status::InvalidField)
        );
        assert_eq!(
            run(&server, command::MOVE_TO_COLOR, &[0, 0, 0x00, 0xff, 0, 0]),
            CommandOutcome::Status(Status::InvalidField)
        );
        assert_eq!(server.xy(), (0x1234, 0x5678));
        assert_eq!(
            run(
                &server,
                command::MOVE_TO_COLOR,
                &[0xff, 0xfe, 0xff, 0xfe, 0, 0]
            ),
            CommandOutcome::Status(Status::Success)
        );
        assert_eq!(server.xy(), (0xfeff, 0xfeff));
        server.set_color_temperature_mireds(370);
        assert!(server.color().0 == ColorMode::ColorTemperature);
    }
}
