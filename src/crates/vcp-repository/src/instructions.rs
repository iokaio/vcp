// SPDX-License-Identifier: Apache-2.0
//! One AGENTS.md-only loader with per-path applicability. Missing files are
//! dependencies too: creating a new nested instruction invalidates the result.
use crate::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub root: RootId,
    pub binding: Revision,
    pub path: String,
    pub observed: Option<FileVersion>,
}
pub struct Instruction {
    pub source: Source,
    pub applies_to: Vec<String>,
    pub precedence: usize,
    pub parent_read_only: bool,
}
pub struct Instructions {
    pub documents: Vec<Instruction>,
    pub probes: Vec<Probe>,
}
impl Root {
    /// Each parent is an explicit instruction-read grant. Only its AGENTS.md is
    /// read; neither that grant nor its text expands ordinary repository scope.
    pub fn instructions(
        &self,
        affected: &[PathBuf],
        parents: &[Root],
        max_bytes: u64,
    ) -> Result<Instructions> {
        if affected.is_empty()
            || affected.len() > 256
            || parents.len() > 32
            || max_bytes == 0
            || max_bytes > 1024 * 1024
        {
            return Err(Error::Limit("instruction inputs"));
        }
        let mut candidates: BTreeMap<(usize, String), Vec<String>> = BTreeMap::new();
        let mut granted: Vec<_> = parents.iter().collect();
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(self.identity.root.clone());
        granted.sort_by_key(|root| root.path.components().count());
        for parent in &granted {
            if !seen.insert(parent.identity.root.clone())
                || parent.identity.workspace != self.identity.workspace
                || parent.path == self.path
                || !self.path.starts_with(&parent.path)
            {
                return Err(Error::Scope(
                    "instruction grant is not an authorized ancestor".into(),
                ));
            }
        }
        granted.push(self);
        for path in affected {
            let normalized = path::relative(path)?;
            for index in 0..parents.len() {
                candidates
                    .entry((index, "AGENTS.md".into()))
                    .or_default()
                    .push(normalized.clone());
            }
            let mut directory = PathBuf::new();
            candidates
                .entry((parents.len(), "AGENTS.md".into()))
                .or_default()
                .push(normalized.clone());
            let parts: Vec<_> = normalized.split('/').collect();
            for component in &parts[..parts.len() - 1] {
                directory.push(component);
                candidates
                    .entry((parents.len(), path::relative(&directory.join("AGENTS.md"))?))
                    .or_default()
                    .push(normalized.clone());
            }
        }
        let mut result = Instructions {
            documents: vec![],
            probes: vec![],
        };
        let mut remaining = max_bytes;
        for ((index, path), mut applies_to) in candidates {
            let root = granted[index];
            let observed = match root.read(Path::new(&path), remaining.max(1)) {
                Ok(source) => {
                    if source.bytes.len() as u64 > remaining {
                        return Err(Error::Limit("required instructions"));
                    }
                    std::str::from_utf8(&source.bytes)
                        .map_err(|_| Error::Unsupported("non-UTF8 instructions"))?;
                    remaining -= source.bytes.len() as u64;
                    let version = source.version.clone();
                    applies_to.sort();
                    applies_to.dedup();
                    result.documents.push(Instruction {
                        source,
                        applies_to,
                        precedence: index * 256 + Path::new(&path).components().count(),
                        parent_read_only: index < parents.len(),
                    });
                    Some(version)
                }
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            result.probes.push(Probe {
                root: root.identity.root.clone(),
                binding: root.identity.binding,
                path,
                observed,
            });
        }
        result.documents.sort_by(|a, b| {
            (a.precedence, &a.source.version.path).cmp(&(b.precedence, &b.source.version.path))
        });
        Ok(result)
    }
}
impl Instructions {
    pub fn revalidate(&self, roots: &[Root]) -> Result<()> {
        revalidate_probes(&self.probes, roots)
    }
}
pub fn revalidate_probes(probes: &[Probe], roots: &[Root]) -> Result<()> {
    for probe in probes {
        let root = roots
            .iter()
            .find(|root| root.identity.root == probe.root && root.identity.binding == probe.binding)
            .ok_or(Error::Stale)?;
        match &probe.observed {
            Some(version) => root.revalidate(version)?,
            None => match root.hold(Some(Path::new(&probe.path)), false) {
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
                _ => return Err(Error::Stale),
            },
        }
    }
    Ok(())
}
