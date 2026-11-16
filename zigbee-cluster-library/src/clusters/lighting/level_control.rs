//! Level Control Cluster
//!
//! See ZCL Section 5.2.

use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::reporting::AttributeReporting;
use crate::server::ClusterCommand;
use crate::server::ClusterServer;
use crate::server::CommandOutcome;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;
use crate::types::integers::Uint8;
use crate::types::integers::Uint16;

pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0008), "LevelControl");
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

pub const CURRENT_LEVEL: Attribute<Uint8, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "CurrentLevel");
pub const REMAINING_TIME: Attribute<Uint16> =
    CLUSTER.attribute(AttributeId(0x0001), "RemainingTime");
pub const MIN_LEVEL: Attribute<Uint8> = CLUSTER.attribute(AttributeId(0x0002), "MinLevel");
pub const MAX_LEVEL: Attribute<Uint8> = CLUSTER.attribute(AttributeId(0x0003), "MaxLevel");

const ATTRIBUTES: &[AttrInfo] = &[
    CURRENT_LEVEL.attr_info(),
    REMAINING_TIME.attr_info(),
    MIN_LEVEL.attr_info(),
    MAX_LEVEL.attr_info(),
];

pub mod attribute {
    pub const CURRENT_LEVEL: u16 = super::CURRENT_LEVEL.id().0;
    pub const REMAINING_TIME: u16 = super::REMAINING_TIME.id().0;
    pub const MIN_LEVEL: u16 = super::MIN_LEVEL.id().0;
    pub const MAX_LEVEL: u16 = super::MAX_LEVEL.id().0;
}

pub mod command {
    pub const MOVE_TO_LEVEL: u8 = 0x00;
    pub const STEP: u8 = 0x02;
    pub const STOP: u8 = 0x03;
    pub const MOVE_TO_LEVEL_WITH_ON_OFF: u8 = 0x04;
    pub const STEP_WITH_ON_OFF: u8 = 0x06;
    pub const STOP_WITH_ON_OFF: u8 = 0x07;
}

const LEVEL_MAX: u8 = 0xfe;

/// Single-light Level Control server for instantaneous move-to and step commands.
pub struct LevelControlServer<'a> {
    current_level: AtomicU8,
    on_off_change: AtomicU8,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> LevelControlServer<'a> {
    pub const fn new(initial_level: u8) -> Self {
        Self {
            current_level: AtomicU8::new(if initial_level > LEVEL_MAX {
                LEVEL_MAX
            } else {
                initial_level
            }),
            on_off_change: AtomicU8::new(0),
            reporting: None,
        }
    }

    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    pub fn current_level(&self) -> u8 {
        self.current_level.load(Ordering::Relaxed)
    }

    pub fn set_current_level(&self, level: u8) {
        self.current_level
            .store(level.min(LEVEL_MAX), Ordering::Relaxed);
    }

    /// Consume the On/Off effect requested by the last With-On/Off command.
    pub fn take_on_off_change(&self) -> Option<bool> {
        match self.on_off_change.swap(0, Ordering::Relaxed) {
            1 => Some(false),
            2 => Some(true),
            _ => None,
        }
    }

    fn set_from_command(&self, level: u8, with_on_off: bool) -> CommandOutcome {
        if level == u8::MAX {
            return CommandOutcome::Status(Status::InvalidField);
        }
        let level = level.min(LEVEL_MAX);
        self.current_level.store(level, Ordering::Relaxed);
        let effect = if with_on_off {
            if level == 0 { 1 } else { 2 }
        } else {
            0
        };
        self.on_off_change.store(effect, Ordering::Relaxed);
        CommandOutcome::Status(Status::Success)
    }

    fn step(&self, mode: u8, amount: u8, with_on_off: bool) -> CommandOutcome {
        let current = self.current_level();
        let level = match mode {
            0 => current.saturating_add(amount).min(LEVEL_MAX),
            1 => current.saturating_sub(amount),
            _ => return CommandOutcome::Status(Status::InvalidField),
        };
        self.set_current_level(level);
        let effect = if with_on_off {
            if level == 0 { 1 } else { 2 }
        } else {
            0
        };
        self.on_off_change.store(effect, Ordering::Relaxed);
        CommandOutcome::Status(Status::Success)
    }
}

impl ClusterServer for LevelControlServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        ATTRIBUTES
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        let encoded = match id.0 {
            attribute::CURRENT_LEVEL => {
                CURRENT_LEVEL.encode(Uint8(self.current_level()), out, offset)
            }
            attribute::REMAINING_TIME => REMAINING_TIME.encode(Uint16(0), out, offset),
            attribute::MIN_LEVEL => MIN_LEVEL.encode(Uint8(0), out, offset),
            attribute::MAX_LEVEL => MAX_LEVEL.encode(Uint8(LEVEL_MAX), out, offset),
            _ => return Status::UnsupportedAttribute,
        };
        encoded.map_or_else(|_| Status::InsufficientSpace, |_| Status::Success)
    }

    fn reporting(&self) -> Option<&dyn AttributeReporting> {
        self.reporting
    }

    fn command(&self, command: ClusterCommand<'_>, _out: &mut [u8]) -> CommandOutcome {
        match command.id.0 {
            command::MOVE_TO_LEVEL | command::MOVE_TO_LEVEL_WITH_ON_OFF => {
                let Some(level) = command.data.get(..3).and_then(|data| data.first()).copied()
                else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                self.set_from_command(level, command.id.0 == command::MOVE_TO_LEVEL_WITH_ON_OFF)
            }
            command::STEP | command::STEP_WITH_ON_OFF => {
                let Some(data) = command.data.get(..4) else {
                    return CommandOutcome::Status(Status::MalformedCommand);
                };
                self.step(data[0], data[1], command.id.0 == command::STEP_WITH_ON_OFF)
            }
            command::STOP | command::STOP_WITH_ON_OFF => {
                self.on_off_change.store(0, Ordering::Relaxed);
                CommandOutcome::Status(Status::Success)
            }
            _ => CommandOutcome::Status(Status::UnsupCommand),
        }
    }
}

impl ClusterRequestHandler for LevelControlServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::CommandId;

    fn run(server: &LevelControlServer<'_>, id: u8, data: &[u8]) -> CommandOutcome {
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
    fn move_to_level_sets_level_and_on_off_effect() {
        let server = LevelControlServer::new(128);
        assert_eq!(
            run(&server, command::MOVE_TO_LEVEL_WITH_ON_OFF, &[200, 0, 0]),
            CommandOutcome::Status(Status::Success)
        );
        assert_eq!(server.current_level(), 200);
        assert_eq!(server.take_on_off_change(), Some(true));

        assert_eq!(
            run(&server, command::MOVE_TO_LEVEL_WITH_ON_OFF, &[0, 0, 0]),
            CommandOutcome::Status(Status::Success)
        );
        assert_eq!(server.current_level(), 0);
        assert_eq!(server.take_on_off_change(), Some(false));
    }

    #[test]
    fn current_level_encodes_as_uint8() {
        let server = LevelControlServer::new(128);
        let mut out = [0u8; 4];
        let mut offset = 0;
        assert_eq!(
            server.encode_value(CURRENT_LEVEL.id(), &mut out, &mut offset),
            Status::Success
        );
        assert_eq!(&out[..offset], &[0x20, 128]);
    }

    #[test]
    fn step_command_adjusts_level_within_the_supported_range() {
        let server = LevelControlServer::new(128);
        assert_eq!(
            run(&server, command::STEP, &[0, 20, 0, 0]),
            CommandOutcome::Status(Status::Success)
        );
        assert_eq!(server.current_level(), 148);

        assert_eq!(
            run(&server, command::STEP_WITH_ON_OFF, &[1, 200, 0, 0]),
            CommandOutcome::Status(Status::Success)
        );
        assert_eq!(server.current_level(), 0);
        assert_eq!(server.take_on_off_change(), Some(false));
    }

    #[test]
    fn move_to_level_requires_its_transition_time() {
        let server = LevelControlServer::new(128);
        assert_eq!(
            run(&server, command::MOVE_TO_LEVEL, &[64]),
            CommandOutcome::Status(Status::MalformedCommand)
        );
        assert_eq!(server.current_level(), 128);
    }
}
