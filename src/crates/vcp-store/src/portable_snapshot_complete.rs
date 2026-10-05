// SPDX-License-Identifier: Apache-2.0
//! Complete neutral-history/2 data archive composition. This internal evaluator
//! requires an actual pinned native snapshot for capture and private destination
//! staging for import. It does not transfer runnable host authority.
use super::{
    artifacts::Artifacts,
    canonical::Canonical,
    inputs::InputArchive,
    wire::{self, IndexedPages, ObjectSet, UntrustedObjects},
};
use crate::{
    artifact::Spool,
    contract::Collection,
    durable_owner::DurableOwner,
    history_index::Pages,
    snapshot_inputs::{Coverage, Inputs},
    CurrentStateView, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    ArtifactId, WorkspaceId,
};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Archive {
    format: String,
    workspace: WorkspaceId,
    authority: String,
    canonical: Canonical,
    artifacts: Artifacts,
    inputs: InputArchive,
    coverage_digest: String,
}
pub(crate) struct Restored {
    pub(crate) owner: DurableOwner,
    pub(crate) spool: Spool,
    pub(crate) inputs: Inputs,
    pub(crate) coverage: Coverage,
    archive: Archive,
}
impl Restored {
    pub(crate) async fn stage_artifacts(
        &self,
        source: &mut impl Pages,
        root: &Path,
        forbidden: &[PathBuf],
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Spool> {
        self.archive
            .artifacts
            .stage(
                self.owner.semantic().current().into(),
                &self.archive.workspace,
                source,
                root,
                forbidden,
                crate::artifact::DEFAULT_ARTIFACT_LIMIT,
                check,
            )
            .await
    }
}
impl Archive {
    /// Caller must hold PinnedDurableSnapshot and its artifact pins for the
    /// entire operation. A decoded root or arbitrary same-watermark State is
    /// never a substitute for that actual native owner capability.
    pub(crate) async fn capture(
        owner: &DurableOwner,
        source: &mut impl Pages,
        scratch: &mut impl Pages,
        workspace: &WorkspaceId,
        spool: &Spool,
        forbidden: &[PathBuf],
        inputs: &Inputs,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        let admitted = crate::private_paths::Directory::open(spool.root(), forbidden)?;
        Self::capture_admitted(
            owner, source, scratch, workspace, spool, &admitted, forbidden, inputs, check,
        )
        .await
    }
    pub(crate) async fn capture_admitted(
        owner: &DurableOwner,
        source: &mut impl Pages,
        scratch: &mut impl Pages,
        workspace: &WorkspaceId,
        spool: &Spool,
        admitted: &crate::private_paths::Directory,
        forbidden: &[PathBuf],
        inputs: &Inputs,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        let current = owner.semantic().current().into();
        let coverage = validate_inputs(inputs, current, workspace, spool, check)?;
        let canonical = Canonical::capture(owner, source, scratch, workspace, check).await?;
        let artifacts = Artifacts::capture_admitted(
            current, workspace, spool, admitted, forbidden, scratch, check,
        )
        .await?;
        let inputs = InputArchive::capture(inputs, scratch, check).await?;
        let archive = Self {
            format: wire::FORMAT.into(),
            workspace: workspace.clone(),
            authority: "historical_only_rebind_required".into(),
            canonical,
            artifacts,
            inputs,
            coverage_digest: digest_bytes(&canonical_bytes(&coverage)?),
        };
        archive.root()?;
        Ok(archive)
    }
    pub(crate) fn root(&self) -> Result<Vec<u8>> {
        let bytes = canonical_bytes(self)?;
        if bytes.len() > 128 * 1024 {
            return Err(Error::Limit("neutral root descriptor"));
        }
        Ok(bytes)
    }
    /// Rebuild the transport inventory from reachable roots, not from every
    /// object written while creating persistent indexes in scratch storage.
    pub(crate) async fn collect(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<ObjectSet> {
        self.validate()?;
        let mut indexed = IndexedPages::new(destination);
        self.canonical
            .copy_objects(source, &mut indexed, check)
            .await?;
        self.artifacts
            .copy_objects(source, &mut indexed, check)
            .await?;
        self.inputs
            .copy_objects(source, &mut indexed, check)
            .await?;
        check()?;
        indexed.finish()
    }
    /// No files or replay result escape this function on validation failure.
    /// Private destination prefixes may exist; caller retains their private pin
    /// and must not publish them without the existing restore authority gate.
    pub(crate) async fn restore(
        untrusted: UntrustedObjects,
        source: &mut impl Pages,
        replayed: &mut impl Pages,
        closure: &mut impl Pages,
        spool_root: &Path,
        forbidden: &[PathBuf],
        artifact_limit: u64,
        expected_workspace: &WorkspaceId,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Restored> {
        check()?;
        if untrusted.root.len() > 128 * 1024 {
            return Err(Error::Limit("neutral root descriptor"));
        }
        let archive: Self = serde_json::from_slice(&untrusted.root)?;
        if archive.root()? != untrusted.root {
            return Err(Error::Corruption("neutral root encoding"));
        }
        archive.validate()?;
        if &archive.workspace != expected_workspace {
            return Err(Error::Access);
        }
        let owner = archive.canonical.replay(source, replayed, check).await?;
        let current = owner.semantic().current().into();
        let spool = archive
            .artifacts
            .stage(
                current,
                expected_workspace,
                source,
                spool_root,
                forbidden,
                artifact_limit,
                check,
            )
            .await?;
        let inputs = archive.inputs.read(source, check).await?;
        let coverage = validate_inputs(&inputs, current, expected_workspace, &spool, check)?;
        if digest_bytes(&canonical_bytes(&coverage)?) != archive.coverage_digest {
            return Err(Error::Corruption("archive coverage differs"));
        }
        let reachable = archive.collect(source, closure, check).await?;
        untrusted
            .objects
            .verify_same(source, &reachable, closure, check)
            .await?;
        check()?;
        Ok(Restored {
            owner,
            spool,
            inputs,
            coverage,
            archive,
        })
    }
    fn validate(&self) -> Result<()> {
        if self.format != wire::FORMAT || self.authority != "historical_only_rebind_required" {
            return Err(Error::Incompatible);
        }
        if self.canonical.workspace() != &self.workspace
            || self.coverage_digest.len() != 64
            || !self
                .coverage_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Corruption("neutral root scope or coverage"));
        }
        Ok(())
    }
}
fn validate_inputs(
    inputs: &Inputs,
    current: CurrentStateView<'_>,
    workspace: &WorkspaceId,
    spool: &Spool,
    check: &dyn Fn() -> Result<()>,
) -> Result<Coverage> {
    inputs.validate_with(current, workspace, &|id: &ArtifactId| {
        check()?;
        let descriptor: ArtifactDescriptor = current
            .record(Collection::Artifact, id.as_str(), workspace)?
            .decode()?;
        if descriptor.state != CaptureState::Complete || descriptor.length.get() > 4 * 1024 * 1024 {
            return Err(Error::Unavailable(
                "archive input incomplete or beyond bound",
            ));
        }
        let mut bytes = Vec::new();
        spool.read(&descriptor, &mut bytes)?;
        Ok(bytes)
    })
}
#[cfg(test)]
#[path = "portable_snapshot_complete_tests.rs"]
mod tests;
