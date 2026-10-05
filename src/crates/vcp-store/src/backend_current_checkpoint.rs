// SPDX-License-Identifier: Apache-2.0
//! Immutable current-owner checkpoints attest to a replayed cut; they never
//! supply state or replace any original commit or authenticated history page.
use super::*;
use vcp_domain::Watermark;

const MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CurrentCheckpoint {
    version: u32,
    pub(super) watermark: Watermark,
    owner: String,
    current: String,
    chain: String,
    end: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal {
    version: u32,
    watermark: Watermark,
    file: String,
    sha256: String,
}

fn name(watermark: Watermark, extension: &str) -> String {
    format!("history-checkpoint-{:020}.{extension}", watermark.get())
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit("current checkpoint bytes"));
    }
    if path.try_exists()? {
        if read_bounded(path, MAX_BYTES)? != bytes {
            return Err(Error::Corruption("current checkpoint collision"));
        }
        return Ok(());
    }
    immutable_file(path, bytes)
}

impl CurrentCheckpoint {
    pub(super) fn end(&self) -> Result<u64> {
        self.end
            .parse()
            .map_err(|_| Error::Corruption("current checkpoint extent"))
    }

    fn from_owner(owner: &DurableOwner, chain: &str, end: u64) -> Result<Self> {
        Ok(Self {
            version: 1,
            watermark: owner.semantic().current().watermark,
            owner: owner.identity().to_owned(),
            current: owner.semantic().current().projection_digest()?,
            chain: chain.to_owned(),
            end: end.to_string(),
        })
    }

    pub(super) fn verify(
        &self,
        owner: &DurableOwner,
        chain: &str,
        end: u64,
        diagnostics: &mut crate::StoreDiagnostics,
    ) -> Result<()> {
        let started = Instant::now();
        let result = Self::from_owner(owner, chain, end).and_then(|expected| {
            if self != &expected {
                Err(Error::Corruption(
                    "current checkpoint differs from canonical history",
                ))
            } else {
                Ok(())
            }
        });
        diagnostics
            .checkpoint_verification
            .record(started, result.is_ok());
        diagnostics.checkpoint_state_comparisons =
            diagnostics.checkpoint_state_comparisons.saturating_add(1);
        result
    }
}

impl Journal {
    pub(crate) fn checkpoint_current(&mut self, owner: &DurableOwner) -> Result<()> {
        let checkpoint =
            CurrentCheckpoint::from_owner(owner, &self.chain, self.file.metadata()?.len())?;
        let bytes = canonical_bytes(&checkpoint)?;
        let file = name(checkpoint.watermark, "json");
        publish(&self.root.join(&file), &bytes)?;
        let seal = Seal {
            version: 1,
            watermark: checkpoint.watermark,
            file,
            sha256: digest_bytes(&bytes),
        };
        publish(
            &self.root.join(name(checkpoint.watermark, "active")),
            &canonical_bytes(&seal)?,
        )
    }

    pub(super) fn read_current_checkpoint(&self) -> Result<Option<CurrentCheckpoint>> {
        let mut latest: Option<(Watermark, PathBuf)> = None;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();
            if !filename.starts_with("history-checkpoint-") || !filename.ends_with(".active") {
                continue;
            }
            let value = filename
                .strip_prefix("history-checkpoint-")
                .and_then(|s| s.strip_suffix(".active"));
            let watermark = value
                .and_then(|s| s.parse::<u64>().ok())
                .map(Watermark::new)
                .ok_or(Error::Corruption("current checkpoint name"))?;
            if filename != name(watermark, "active") {
                return Err(Error::Corruption("current checkpoint name"));
            }
            if latest.as_ref().is_none_or(|(prior, _)| watermark > *prior) {
                latest = Some((watermark, entry.path()));
            }
        }
        let Some((watermark, path)) = latest else {
            return Ok(None);
        };
        let seal: Seal = serde_json::from_slice(&read_bounded(&path, MAX_BYTES)?)?;
        if seal.version != 1 || seal.watermark != watermark || seal.file != name(watermark, "json")
        {
            return Err(Error::Corruption("current checkpoint seal"));
        }
        let bytes = read_bounded(&self.root.join(&seal.file), MAX_BYTES)?;
        if digest_bytes(&bytes) != seal.sha256 {
            return Err(Error::Corruption("current checkpoint digest"));
        }
        let checkpoint: CurrentCheckpoint = serde_json::from_slice(&bytes)?;
        if checkpoint.version != 1 || checkpoint.watermark != watermark {
            return Err(Error::Corruption("current checkpoint descriptor"));
        }
        Ok(Some(checkpoint))
    }
}

impl open::Opened {
    pub(crate) fn checkpoint(&mut self) -> Result<()> {
        self.ensure_healthy()?;
        let started = Instant::now();
        let result = self.backend.checkpoint_current(&self.owner);
        self.diagnostics.checkpoint.record(started, result.is_ok());
        result
    }
}

#[cfg(test)]
#[path = "backend_current_checkpoint_tests.rs"]
mod tests;
