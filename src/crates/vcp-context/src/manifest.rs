// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use vcp_domain::{artifact::ArtifactDescriptor, workspace::Scope, *};
use vcp_repository::{instructions::Probe, FileVersion};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid context: {0}")]
    Invalid(&'static str),
    #[error("required context cannot fit the selected model envelope")]
    Capacity,
    #[error("context dependency changed before admission")]
    Stale,
    #[error("model cannot preserve required context semantics: {0}")]
    Incompatible(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revisions {
    pub scope: Scope,
    pub steering: SteeringRevision,
    pub policy: PolicyRevision,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub binding: Revision,
    pub instructions: Revision,
    pub tools: Revision,
    pub skills: Revision,
    pub memory: Revision,
    pub task_state: Revision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Operating,
    Objective,
    Constraint,
    ProjectInstruction,
    Skill,
    TaskState,
    Evidence,
    History,
    ToolCall,
    ToolResult,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Operating,
    User,
    Project,
    ActiveSkill,
    Observed,
    Untrusted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        output: String,
    },
}
impl Content {
    pub fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Text { text } => Ok(text.as_bytes().to_vec()),
            _ => Ok(vcp_protocol::canonical_bytes(self)?),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: String,
    pub scope: Scope,
    pub kind: Kind,
    pub trust: Trust,
    pub artifact: ArtifactId,
    pub source_hash: String,
    pub source_length: ByteCount,
    pub start: ByteCount,
    pub end: ByteCount,
    pub content: Content,
    pub mandatory: bool,
    pub rank: u32,
    pub reason: String,
    pub applicable_paths: Vec<String>,
    pub file: Option<FileVersion>,
}
impl Part {
    pub fn captured_text(
        id: String,
        kind: Kind,
        trust: Trust,
        descriptor: &ArtifactDescriptor,
        bytes: &[u8],
        mandatory: bool,
        rank: u32,
        reason: String,
    ) -> Result<Self> {
        descriptor
            .validate()
            .map_err(|_| Error::Invalid("artifact descriptor"))?;
        if descriptor.state != vcp_domain::artifact::CaptureState::Complete
            || descriptor.sha256 != vcp_protocol::digest_bytes(bytes)
            || descriptor.length.get() != bytes.len() as u64
        {
            return Err(Error::Invalid("captured source identity"));
        }
        let content = Content::Text {
            text: std::str::from_utf8(bytes)
                .map_err(|_| Error::Invalid("binary context requires an explicit conversion"))?
                .into(),
        };
        Ok(Self {
            id,
            scope: descriptor.spec.scope.clone(),
            kind,
            trust,
            artifact: descriptor.spec.id.clone(),
            source_hash: vcp_protocol::digest_bytes(bytes),
            source_length: descriptor.length,
            start: ByteCount::ZERO,
            end: ByteCount::new(bytes.len() as u64),
            content,
            mandatory,
            rank,
            reason,
            applicable_paths: vec![],
            file: None,
        })
    }
    pub fn required(&self) -> bool {
        self.mandatory || !matches!(self.kind, Kind::Evidence | Kind::History)
    }
    pub fn validate(&self, scope: &Scope) -> Result<()> {
        if &self.scope != scope
            || self.id.is_empty()
            || self.id.len() > 128
            || self.reason.is_empty()
            || self.reason.len() > 4096
            || self.source_hash.len() != 64
            || !self.source_hash.bytes().all(|b| b.is_ascii_hexdigit())
            || self.end > self.source_length
            || self.end.get().checked_sub(self.start.get())
                != Some(self.content.bytes()?.len() as u64)
        {
            return Err(Error::Invalid("part identity/scope/range"));
        }
        if self.start == ByteCount::ZERO
            && self.end == self.source_length
            && vcp_protocol::digest_bytes(&self.content.bytes()?) != self.source_hash
        {
            return Err(Error::Invalid("full source bytes differ from capture"));
        }
        if self.applicable_paths.len() > 256
            || self
                .applicable_paths
                .iter()
                .any(|path| vcp_repository::path::relative(std::path::Path::new(path)).is_err())
        {
            return Err(Error::Invalid("instruction applicability"));
        }
        let valid = match self.kind {
            Kind::Operating => self.trust == Trust::Operating,
            Kind::Objective | Kind::Constraint => self.trust == Trust::User,
            Kind::ProjectInstruction => {
                self.trust == Trust::Project && !self.applicable_paths.is_empty()
            }
            Kind::Skill => self.trust == Trust::ActiveSkill,
            Kind::TaskState => self.trust == Trust::Observed,
            Kind::Evidence | Kind::History | Kind::ToolCall | Kind::ToolResult => {
                matches!(self.trust, Trust::Observed | Trust::Untrusted)
            }
        };
        if !valid {
            return Err(Error::Invalid("trust class does not match source kind"));
        }
        if matches!(self.content, Content::ToolCall { .. }) != (self.kind == Kind::ToolCall)
            || matches!(self.content, Content::ToolResult { .. }) != (self.kind == Kind::ToolResult)
        {
            return Err(Error::Invalid("tool content kind"));
        }
        if let Some(file) = &self.file {
            if file.sha256 != self.source_hash || file.bytes != self.source_length {
                return Err(Error::Invalid("file provenance"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub model: String,
    pub catalog: String,
    pub compatibility: String,
    pub context: Units,
    pub output: Units,
    pub overhead: Units,
    pub margin: Units,
    pub supports_tools: bool,
    pub preserves_trust: bool,
}
impl Envelope {
    pub fn input_capacity(&self) -> Result<u64> {
        if self.model.is_empty()
            || self.catalog.is_empty()
            || self.compatibility.is_empty()
            || self.output == Units::ZERO
            || !self.preserves_trust
        {
            return Err(Error::Incompatible("envelope/role capability"));
        }
        self.context
            .get()
            .checked_sub(self.output.get())
            .and_then(|n| n.checked_sub(self.overhead.get()))
            .and_then(|n| n.checked_sub(self.margin.get()))
            .ok_or(Error::Capacity)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Excluded {
    pub id: String,
    pub reason: String,
    pub start: ByteCount,
    pub end: ByteCount,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub revisions: Revisions,
    pub envelope: Envelope,
    pub included: Vec<Part>,
    pub excluded: Vec<Excluded>,
    pub instruction_probes: Vec<Probe>,
    pub schemas_sha256: String,
    pub request_sha256: String,
    pub input_estimate: Units,
    pub estimate_method: String,
    pub estimated: bool,
}
pub struct Sealed {
    pub manifest: Manifest,
    body: Vec<u8>,
    digest: String,
}
/// A non-serializable proof that the selected views were compared with their
/// captured source bytes. It conveys no tool/policy or budget authority.
pub struct VerifiedContext {
    sealed: Sealed,
}
impl VerifiedContext {
    /// Recheck the same captured bytes and selected ranges without duplicating
    /// or reconstructing a context proof shared by concurrent host observers.
    pub fn reverify_captures(
        &self,
        resolve: impl FnMut(&Scope, &ArtifactId, u64) -> Result<(ArtifactDescriptor, Vec<u8>)>,
    ) -> Result<()> {
        self.sealed.check_captures(resolve)
    }
    pub fn sealed(&self) -> &Sealed {
        &self.sealed
    }
    pub fn into_sealed(self) -> Sealed {
        self.sealed
    }
}
impl Sealed {
    pub(crate) fn new(manifest: Manifest, body: Vec<u8>) -> Result<Self> {
        let digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&manifest)?);
        Ok(Self {
            manifest,
            body,
            digest,
        })
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn manifest_digest(&self) -> &str {
        &self.digest
    }
    /// The host resolver enforces current artifact access and the supplied
    /// length bound. Aggregate source verification is limited to 64 MiB.
    pub fn verify_captures(
        self,
        resolve: impl FnMut(&Scope, &ArtifactId, u64) -> Result<(ArtifactDescriptor, Vec<u8>)>,
    ) -> Result<VerifiedContext> {
        self.check_captures(resolve)?;
        Ok(VerifiedContext { sealed: self })
    }
    fn check_captures(
        &self,
        mut resolve: impl FnMut(&Scope, &ArtifactId, u64) -> Result<(ArtifactDescriptor, Vec<u8>)>,
    ) -> Result<()> {
        if vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&self.manifest)?)
            != self.digest
            || vcp_protocol::digest_bytes(&self.body) != self.manifest.request_sha256
        {
            return Err(Error::Stale);
        }
        let mut unique = std::collections::BTreeMap::new();
        let mut total = 0u64;
        for part in &self.manifest.included {
            if let Some((length, hash)) = unique.get(&part.artifact) {
                if *length != part.source_length.get() || hash != &part.source_hash {
                    return Err(Error::Invalid("conflicting artifact identity"));
                }
            } else {
                total = total
                    .checked_add(part.source_length.get())
                    .ok_or(Error::Capacity)?;
                if total > 64 * 1024 * 1024 {
                    return Err(Error::Capacity);
                }
                unique.insert(
                    part.artifact.clone(),
                    (part.source_length.get(), part.source_hash.clone()),
                );
            }
        }
        for (id, (length, hash)) in unique {
            let (descriptor, bytes) = resolve(&self.manifest.revisions.scope, &id, length)?;
            descriptor
                .validate()
                .map_err(|_| Error::Invalid("captured artifact descriptor"))?;
            if descriptor.spec.scope != self.manifest.revisions.scope
                || descriptor.spec.id != id
                || descriptor.state != vcp_domain::artifact::CaptureState::Complete
                || descriptor.length.get() != length
                || descriptor.sha256 != hash
                || bytes.len() as u64 != length
                || vcp_protocol::digest_bytes(&bytes) != hash
            {
                return Err(Error::Invalid("captured artifact identity"));
            }
            for part in self
                .manifest
                .included
                .iter()
                .filter(|part| part.artifact == id)
            {
                if bytes.get(part.start.get() as usize..part.end.get() as usize)
                    != Some(part.content.bytes()?.as_slice())
                {
                    return Err(Error::Invalid("selected range differs from captured bytes"));
                }
            }
        }
        Ok(())
    }
    /// The controller orders this check with canonical authority/admission.
    /// It cannot retract a request whose prior send admission already committed.
    pub fn revalidate(
        &self,
        current: &Revisions,
        envelope: &Envelope,
        schemas: &serde_json::Value,
        sources_current: impl FnOnce(&Manifest) -> bool,
    ) -> Result<()> {
        if current != &self.manifest.revisions
            || envelope != &self.manifest.envelope
            || vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(schemas)?)
                != self.manifest.schemas_sha256
            || vcp_protocol::digest_bytes(&self.body) != self.manifest.request_sha256
            || vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&self.manifest)?)
                != self.digest
            || !sources_current(&self.manifest)
        {
            return Err(Error::Stale);
        }
        Ok(())
    }
}
