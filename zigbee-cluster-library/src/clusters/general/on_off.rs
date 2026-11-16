//! On/Off Cluster
//!
//! See ZCL Section 3.8
//!
//! Switches a device between its "on" and "off" states. [`OnOffServer`] owns
//! the `OnOff` attribute and answers the Off/On/Toggle commands; the
//! application reads [`OnOffServer::on_off`] to drive the hardware.

use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use zigbee_core::zdo::ClusterReply;
use zigbee_core::zdo::ClusterRequest;
use zigbee_core::zdo::ClusterRequestHandler;

use crate::frame::Status;
use crate::reporting::AttributeReporting;
use crate::server::ClusterCommand;
use crate::server::ClusterServer;
use crate::server::CommandOutcome;
use crate::types::codec::Bool;
use crate::types::descriptors::AttrInfo;
use crate::types::descriptors::Attribute;
use crate::types::descriptors::Cluster;
use crate::types::descriptors::ReadOnly;
use crate::types::descriptors::Reportable;
use crate::types::ids::AttributeId;
use crate::types::ids::ClusterId;

/// Cluster descriptor (ZCL 3.8).
pub const CLUSTER: Cluster = Cluster::new(ClusterId(0x0006), "OnOff");

/// Cluster identifier (ZCL 3.8), for matching against a received frame.
pub const CLUSTER_ID: u16 = CLUSTER.id().0;

/// `OnOff` (ZCL 3.8.2.2.1), whether the device is on.
///
/// Read-only over the air: it is changed by the cluster's own commands, not by
/// a write.
pub const ON_OFF: Attribute<Bool, ReadOnly, Reportable> =
    CLUSTER.attribute(AttributeId(0x0000), "OnOff");

/// Every attribute this cluster implements, in ascending identifier order
/// (2.5.13.3).
const ATTRIBUTES: &[AttrInfo] = &[ON_OFF.attr_info()];

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptor above, which stays the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `OnOff` (`bool`), whether the device is on.
    pub const ON_OFF: u16 = super::ON_OFF.id().0;
}

/// Cluster-specific command identifiers (ZCL 3.8.2.3).
pub mod command {
    /// Received: enter the off state.
    pub const OFF: u8 = 0x00;
    /// Received: enter the on state.
    pub const ON: u8 = 0x01;
    /// Received: enter whichever state the device is not in.
    pub const TOGGLE: u8 = 0x02;
}

/// On/Off cluster server (ZCL 3.8).
///
/// The mandatory attribute and the three mandatory commands. The server only
/// tracks the state; the application polls [`on_off`](Self::on_off) — or
/// watches it change — and drives the hardware from it.
#[derive(Default)]
pub struct OnOffServer<'a> {
    on_off: AtomicBool,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> OnOffServer<'a> {
    /// A server that starts out off, the state a light powers up in
    /// (ZCL 3.8.2.2.1).
    pub const fn new() -> Self {
        Self {
            on_off: AtomicBool::new(false),
            reporting: None,
        }
    }

    /// Let a coordinator configure reporting, keeping what it asks for in
    /// `store` (ZCL 2.5.7).
    ///
    /// Reporting is mandatory for `OnOff` (ZCL 3.8.2.2.1), so a server that
    /// leaves this unset refuses `Configure Reporting` with
    /// `UNREPORTABLE_ATTRIBUTE` and a strict coordinator will call that a
    /// failed interview step.
    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    /// Whether the device is currently on.
    pub fn on_off(&self) -> bool {
        self.on_off.load(Ordering::Relaxed)
    }

    /// Set the state, as a local control (a switch on the device) would.
    pub fn set_on_off(&self, on: bool) {
        self.on_off.store(on, Ordering::Relaxed);
    }

    /// Flip the state and report what it became.
    pub fn toggle(&self) -> bool {
        !self.on_off.fetch_xor(true, Ordering::Relaxed)
    }
}

impl ClusterServer for OnOffServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        ATTRIBUTES
    }

    fn encode_value(&self, id: AttributeId, out: &mut [u8], offset: &mut usize) -> Status {
        if id.0 != attribute::ON_OFF {
            return Status::UnsupportedAttribute;
        }
        match ON_OFF.encode(self.on_off(), out, offset) {
            Ok(()) => Status::Success,
            Err(_) => Status::InsufficientSpace,
        }
    }

    fn reporting(&self) -> Option<&dyn AttributeReporting> {
        self.reporting
    }

    fn command(&self, command: ClusterCommand<'_>, _out: &mut [u8]) -> CommandOutcome {
        match command.id.0 {
            command::OFF => self.set_on_off(false),
            command::ON => self.set_on_off(true),
            command::TOGGLE => {
                self.toggle();
            }
            _ => return CommandOutcome::Status(Status::UnsupCommand),
        }
        CommandOutcome::Status(Status::Success)
    }
}

impl ClusterRequestHandler for OnOffServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::CommandId;

    fn run(server: &OnOffServer<'_>, id: u8) -> CommandOutcome {
        let mut out = [0u8; 16];
        server.command(
            ClusterCommand {
                id: CommandId(id),
                data: &[],
                sequence_number: 1,
            },
            &mut out,
        )
    }

    #[test]
    fn on_off_and_toggle_move_the_state() {
        let server = OnOffServer::new();
        assert!(!server.on_off());

        assert_eq!(
            run(&server, command::ON),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.on_off());

        assert_eq!(
            run(&server, command::TOGGLE),
            CommandOutcome::Status(Status::Success)
        );
        assert!(!server.on_off());

        assert_eq!(
            run(&server, command::TOGGLE),
            CommandOutcome::Status(Status::Success)
        );
        assert!(server.on_off());

        assert_eq!(
            run(&server, command::OFF),
            CommandOutcome::Status(Status::Success)
        );
        assert!(!server.on_off());
    }

    #[test]
    fn an_unknown_command_is_unsupported() {
        let server = OnOffServer::new();
        assert_eq!(
            run(&server, 0x40),
            CommandOutcome::Status(Status::UnsupCommand)
        );
    }

    #[test]
    fn on_off_encodes_as_a_boolean() {
        let server = OnOffServer::new();
        server.set_on_off(true);

        let mut out = [0u8; 8];
        let mut offset = 0;
        assert_eq!(
            server.encode_value(ON_OFF.id(), &mut out, &mut offset),
            Status::Success
        );
        assert_eq!(&out[..offset], &[0x10, 0x01]);
    }
}
