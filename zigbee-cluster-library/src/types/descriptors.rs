use core::marker::PhantomData;

use super::codec::ZclKind;
use super::ids::AttributeId;
use super::ids::ClusterId;
use super::ids::CommandId;
use super::ids::ManufacturerCode;
use super::ids::TypeId;

pub struct ReadOnly;
pub struct WriteOnly;
pub struct ReadWrite;

pub trait Readable {}
pub trait Writable {}

pub trait AccessTypestate {
    const FLAGS: AccessFlags;
}

impl Readable for ReadOnly {}
impl Readable for ReadWrite {}
impl Writable for WriteOnly {}
impl Writable for ReadWrite {}

impl AccessTypestate for ReadOnly {
    const FLAGS: AccessFlags = AccessFlags::READ;
}
impl AccessTypestate for WriteOnly {
    const FLAGS: AccessFlags = AccessFlags::WRITE;
}
impl AccessTypestate for ReadWrite {
    const FLAGS: AccessFlags = AccessFlags::READ_WRITE;
}

pub struct Reportable;
pub struct NotReportable;

pub trait ReportTypestate {
    const FLAG: AccessFlags;
}

impl ReportTypestate for Reportable {
    const FLAG: AccessFlags = AccessFlags::REPORTABLE;
}
impl ReportTypestate for NotReportable {
    const FLAG: AccessFlags = AccessFlags::EMPTY;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessFlags(u8);

impl AccessFlags {
    pub const READ: Self = Self(0x01);
    pub const WRITE: Self = Self(0x02);
    pub const READ_WRITE: Self = Self(0x03);
    pub const REPORTABLE: Self = Self(0x04);
    pub const EMPTY: Self = Self(0);

    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn is_readable(self) -> bool {
        self.0 & 0x01 != 0
    }
    pub const fn is_writable(self) -> bool {
        self.0 & 0x02 != 0
    }
    pub const fn is_reportable(self) -> bool {
        self.0 & 0x04 != 0
    }
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

pub struct ClientToServer;
pub struct ServerToClient;

pub trait Sendable {}
pub trait Receivable {}

impl Sendable for ClientToServer {}
impl Receivable for ServerToClient {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cluster {
    id: ClusterId,
    manufacturer: Option<ManufacturerCode>,
    name: &'static str,
}

impl Cluster {
    pub const fn new(id: ClusterId, name: &'static str) -> Self {
        Self {
            id,
            manufacturer: None,
            name,
        }
    }

    pub const fn manufacturer_specific(
        id: ClusterId,
        manufacturer: ManufacturerCode,
        name: &'static str,
    ) -> Self {
        Self {
            id,
            manufacturer: Some(manufacturer),
            name,
        }
    }

    pub const fn id(self) -> ClusterId {
        self.id
    }

    pub const fn manufacturer_code(self) -> Option<ManufacturerCode> {
        self.manufacturer
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn attribute<T, Access, Report>(
        self,
        id: AttributeId,
        name: &'static str,
    ) -> Attribute<T, Access, Report> {
        Attribute {
            cluster: self,
            id,
            name,
            _kind: PhantomData,
            _access: PhantomData,
            _report: PhantomData,
        }
    }

    pub const fn command<Payload, Direction>(
        self,
        id: CommandId,
        name: &'static str,
    ) -> Command<Payload, Direction> {
        Command {
            cluster: self,
            id,
            name,
            _payload: PhantomData,
            _direction: PhantomData,
        }
    }
}

pub struct Attribute<T, Access = ReadOnly, Report = NotReportable> {
    cluster: Cluster,
    id: AttributeId,
    name: &'static str,
    _kind: PhantomData<T>,
    _access: PhantomData<Access>,
    _report: PhantomData<Report>,
}

impl<T: ZclKind, A: AccessTypestate, R: ReportTypestate> Attribute<T, A, R> {
    pub const fn id(&self) -> AttributeId {
        self.id
    }

    pub const fn cluster(&self) -> Cluster {
        self.cluster
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn type_id(&self) -> TypeId {
        T::TYPE_ID
    }

    /// Append this attribute's `type | value` pair to `bytes`, for a
    /// `Read Attributes Response` record (2.5.2).
    ///
    /// The type identifier comes from the descriptor, so a value can only be
    /// written with the type the specification gives the attribute.
    pub fn encode(
        &self,
        value: T::Value<'_>,
        bytes: &mut [u8],
        offset: &mut usize,
    ) -> byte::Result<()> {
        T::write_typed(value, bytes, offset)
    }

    /// Decode this attribute's value from a record body, rejecting a type
    /// identifier that is not the one the descriptor declares.
    pub fn decode<'f>(
        &self,
        type_id: TypeId,
        bytes: &'f [u8],
        offset: &mut usize,
    ) -> byte::Result<T::Value<'f>> {
        T::read_typed(type_id, bytes, offset)
    }

    pub const fn access_flags(&self) -> AccessFlags {
        A::FLAGS.union(R::FLAG)
    }

    pub const fn attr_info(&self) -> AttrInfo {
        AttrInfo {
            id: self.id,
            type_id: T::TYPE_ID,
            access: A::FLAGS.union(R::FLAG),
        }
    }
}

/// Writing a `Report Attributes` record is available only for the attributes
/// the specification marks reportable (2.5.11).
impl<T: ZclKind, A: AccessTypestate> Attribute<T, A, Reportable> {
    /// Append this attribute's `Report Attributes` record to `bytes`.
    pub fn report(
        &self,
        value: T::Value<'_>,
        bytes: &mut [u8],
        offset: &mut usize,
    ) -> byte::Result<()> {
        byte::BytesExt::write_with(bytes, offset, self.id.0, byte::ctx::LE)?;
        self.encode(value, bytes, offset)
    }
}

/// Writing a `Write Attributes` record is available only for writable
/// attributes (2.5.3).
impl<T: ZclKind, A: AccessTypestate + Writable, R: ReportTypestate> Attribute<T, A, R> {
    /// Append this attribute's `Write Attributes` record to `bytes`.
    pub fn write(
        &self,
        value: T::Value<'_>,
        bytes: &mut [u8],
        offset: &mut usize,
    ) -> byte::Result<()> {
        byte::BytesExt::write_with(bytes, offset, self.id.0, byte::ctx::LE)?;
        self.encode(value, bytes, offset)
    }
}

impl<T, A, R> core::fmt::Debug for Attribute<T, A, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Attribute")
            .field("cluster", &self.cluster)
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

pub struct Command<Payload, Direction> {
    cluster: Cluster,
    id: CommandId,
    name: &'static str,
    _payload: PhantomData<Payload>,
    _direction: PhantomData<Direction>,
}

impl<Payload, Direction> Command<Payload, Direction> {
    pub const fn id(&self) -> CommandId {
        self.id
    }

    pub const fn cluster(&self) -> Cluster {
        self.cluster
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl<P, D> Clone for Command<P, D> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<P, D> Copy for Command<P, D> {}
impl<P, D> core::fmt::Debug for Command<P, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Command")
            .field("cluster", &self.cluster)
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

/// What a descriptor knows about an attribute once its Rust type is erased.
///
/// This is the form
/// [`ClusterServer::attributes`](crate::server::ClusterServer::attributes)
/// hands out, so `Discover Attributes` and the validation passes of
/// `Write Attributes Undivided` and `Configure Reporting` all work from the
/// descriptors rather than from a second, hand-written list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttrInfo {
    /// Identifier the attribute is addressed by (2.4.1.4).
    pub id: AttributeId,
    /// Wire type its value carries (2.6.2).
    pub type_id: TypeId,
    /// Whether it can be read, written and reported.
    pub access: AccessFlags,
}

/// `(ClusterId, Option<ManufacturerCode>)` pair for ZDO advertisement and frame
/// routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterKey {
    pub id: ClusterId,
    pub manufacturer: Option<ManufacturerCode>,
}

impl ClusterKey {
    pub const fn new(id: ClusterId, manufacturer: Option<ManufacturerCode>) -> Self {
        Self { id, manufacturer }
    }
}

// attribute descriptor for gateways, bridges, and logging tools that process
// attributes without compile-time schema knowledge
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct AttributeDescriptor {
    pub cluster: ClusterId,
    pub manufacturer: Option<ManufacturerCode>,
    pub attribute: AttributeId,
    pub type_id: TypeId,
    pub access: AccessFlags,
    pub name: &'static str,
}
