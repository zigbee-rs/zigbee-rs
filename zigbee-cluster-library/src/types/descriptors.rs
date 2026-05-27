use core::marker::PhantomData;

use super::error::AttrError;
use super::ids::AttributeId;
use super::ids::ClusterId;
use super::ids::CommandId;
use super::ids::ManufacturerCode;
use super::ids::TypeId;
use super::schema::ZclSchema;

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

    pub const fn attribute<S, Access, Report>(
        self,
        id: AttributeId,
        name: &'static str,
    ) -> Attribute<S, Access, Report> {
        Attribute {
            cluster: self,
            id,
            name,
            _schema: PhantomData,
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

pub struct Attribute<S, Access = ReadOnly, Report = NotReportable> {
    cluster: Cluster,
    id: AttributeId,
    name: &'static str,
    _schema: PhantomData<S>,
    _access: PhantomData<Access>,
    _report: PhantomData<Report>,
}

impl<S: ZclSchema, A: AccessTypestate, R: ReportTypestate> Attribute<S, A, R> {
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
        S::TYPE_ID
    }

    pub const fn access_flags(&self) -> AccessFlags {
        A::FLAGS.union(R::FLAG)
    }

    pub const fn attr_info(&self) -> AttrInfo {
        AttrInfo {
            id: self.id,
            type_id: S::TYPE_ID,
            access: A::FLAGS.union(R::FLAG),
        }
    }
}

impl<S, A, R> core::fmt::Debug for Attribute<S, A, R> {
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

/// Attribute metadata returned by `ClusterServer::attribute_list()` for
/// `DiscoverAttributes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttrInfo {
    pub id: AttributeId,
    pub type_id: TypeId,
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

/// Attribute descriptor for gateways, bridges, and logging tools that process
/// attributes without compile-time schema knowledge.
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

/// Encode a typed attribute value into `buf`. Returns `(TypeId,
/// bytes_written)`.
pub fn encode_attr<S: ZclSchema>(
    value: S::Value<'_>,
    buf: &mut [u8],
) -> Result<(TypeId, usize), super::error::ZclError> {
    let n = S::encode(value, buf)?;
    Ok((S::TYPE_ID, n))
}

/// Decode a typed attribute value from `data`. Rejects `TypeId` mismatches and
/// trailing bytes.
pub fn decode_attr<S: ZclSchema>(type_id: TypeId, data: &[u8]) -> Result<S::Value<'_>, AttrError> {
    if type_id != S::TYPE_ID {
        return Err(AttrError::InvalidDataType);
    }
    let (value, used) = S::decode(data).map_err(AttrError::Codec)?;
    if used != data.len() {
        return Err(AttrError::Codec(super::error::ZclError::InvalidLength));
    }
    Ok(value)
}
