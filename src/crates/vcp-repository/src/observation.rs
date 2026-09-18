// SPDX-License-Identifier: Apache-2.0
use crate::{
    discovery::{Exclusion, Limits},
    git::{Git, Observation as GitObservation, Snapshot},
    instructions::Probe,
    *,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub identity: RootIdentity,
    pub git: Option<Snapshot>,
    pub files: Vec<FileVersion>,
    pub ignore_dependencies: Vec<Probe>,
    pub exclusions: Vec<Exclusion>,
    /// Complete within the stated discovery policy. Ignored/generated/link
    /// exclusions are still explicit; this is never proof of their absence.
    pub bounded_scan_complete: bool,
}
pub struct Observation {
    pub manifest: Manifest,
    pub digest: String,
    pub sources: Vec<Source>,
    pub git: Option<GitObservation>,
}
impl Root {
    pub async fn observe(&self, git: Option<&Git>, limits: &Limits) -> Result<Observation> {
        let scan = self.discover(limits)?;
        let git = match git {
            Some(git) => Some(git.observe(self).await?),
            None => None,
        };
        for source in &scan.sources {
            self.revalidate(&source.version)?;
        }
        instructions::revalidate_probes(&scan.ignore_dependencies, std::slice::from_ref(self))?;
        let manifest = Manifest {
            version: 1,
            identity: self.identity.clone(),
            git: git.as_ref().map(|row| row.snapshot.clone()),
            files: scan
                .sources
                .iter()
                .map(|source| source.version.clone())
                .collect(),
            ignore_dependencies: scan.ignore_dependencies,
            exclusions: scan.exclusions,
            bounded_scan_complete: scan.complete,
        };
        let digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&manifest)?);
        Ok(Observation {
            manifest,
            digest,
            sources: scan.sources,
            git,
        })
    }
}
impl Manifest {
    /// Revalidate selected source dependencies instead of rehashing every
    /// unrelated file. Instruction/ignore absence probes remain in the fence.
    pub fn revalidate_selected(&self, root: &Root, selected: &[FileVersion]) -> Result<()> {
        if self.identity != root.identity {
            return Err(Error::Stale);
        }
        instructions::revalidate_probes(&self.ignore_dependencies, std::slice::from_ref(root))?;
        for version in selected {
            if !self.files.contains(version) {
                return Err(Error::Scope("source absent from observed manifest".into()));
            }
            root.revalidate(version)?;
        }
        Ok(())
    }
}
