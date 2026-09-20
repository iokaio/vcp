// SPDX-License-Identifier: Apache-2.0
use super::{
    registration::{self, Registration},
    schema::Schema,
};
use serde::Serialize;

pub const PROTOCOL: &str = "2025-11-25";
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConnectionIdentity {
    registration_digest: String,
    generation: String,
    protocol: String,
    admission_profile: String,
    peer_identity_digest: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ToolIdentity {
    connection: ConnectionIdentity,
    remote_name: String,
    schema_digest: String,
    catalog_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Resource,
    Prompt,
}
/// Checked external content identity; URI/name is data, never local authority.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ContentIdentity {
    connection: ConnectionIdentity,
    kind: ContentKind,
    key: String,
    descriptor_digest: String,
    catalog_revision: u64,
}
impl std::fmt::Debug for ContentIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentIdentity")
            .field("kind", &self.kind)
            .field("catalog_revision", &self.catalog_revision)
            .finish_non_exhaustive()
    }
}
impl ContentIdentity {
    pub(crate) fn new(
        connection: ConnectionIdentity,
        kind: ContentKind,
        key: String,
        descriptor_digest: String,
        catalog_revision: u64,
    ) -> Result<Self, Error> {
        if !registration::sha256(&descriptor_digest)
            || match kind {
                ContentKind::Resource => !super::content::valid_uri(&key),
                ContentKind::Prompt => !registration::remote_name(&key),
            }
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            connection,
            kind,
            key,
            descriptor_digest,
            catalog_revision,
        })
    }
    pub fn connection(&self) -> &ConnectionIdentity {
        &self.connection
    }
    pub fn kind(&self) -> ContentKind {
        self.kind
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn descriptor_digest(&self) -> &str {
        &self.descriptor_digest
    }
    pub fn catalog_revision(&self) -> u64 {
        self.catalog_revision
    }
    pub fn digest(&self) -> Result<String, Error> {
        vcp_protocol::canonical_bytes(self)
            .map(|bytes| vcp_protocol::digest_bytes(&bytes))
            .map_err(|_| Error::Invalid)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid MCP connection identity")]
    Invalid,
    #[error("unsupported MCP protocol revision")]
    Protocol,
}
impl ConnectionIdentity {
    /// Allocate a fresh UUID generation for every successful handshake, including
    /// reconnects after owner reopen. No caller-supplied counter or restored value.
    pub fn new(
        registration: &Registration,
        negotiated: &str,
        peer: Option<String>,
    ) -> Result<Self, Error> {
        if negotiated != PROTOCOL {
            return Err(Error::Protocol);
        }
        if peer.as_ref().is_some_and(|p| !registration::sha256(p)) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            registration_digest: registration.digest().map_err(|_| Error::Invalid)?,
            generation: vcp_domain::ExecutionId::new().to_string(),
            protocol: PROTOCOL.to_owned(),
            admission_profile: super::schema::PROFILE.to_owned(),
            peer_identity_digest: peer,
        })
    }
    pub fn admission_profile(&self) -> &str {
        &self.admission_profile
    }
    pub fn registration_digest(&self) -> &str {
        &self.registration_digest
    }
    pub fn generation(&self) -> &str {
        &self.generation
    }
}
impl ToolIdentity {
    pub fn new(
        connection: ConnectionIdentity,
        remote_name: String,
        schema: &Schema,
    ) -> Result<Self, Error> {
        Self::for_catalog(connection, remote_name, schema, 0)
    }
    pub(crate) fn for_catalog(
        connection: ConnectionIdentity,
        remote_name: String,
        schema: &Schema,
        catalog_revision: u64,
    ) -> Result<Self, Error> {
        if !registration::remote_name(&remote_name) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            connection,
            remote_name,
            schema_digest: schema.digest().to_owned(),
            catalog_revision,
        })
    }
    pub fn connection(&self) -> &ConnectionIdentity {
        &self.connection
    }
    pub fn remote_name(&self) -> &str {
        &self.remote_name
    }
    pub fn schema_digest(&self) -> &str {
        &self.schema_digest
    }
    pub fn digest(&self) -> Result<String, Error> {
        vcp_protocol::canonical_bytes(self)
            .map(|bytes| vcp_protocol::digest_bytes(&bytes))
            .map_err(|_| Error::Invalid)
    }
}
// Deliberately no Deserialize on checked identities. Persist a raw record and
// reconstruct against current registration/connection/schema during reopening.
