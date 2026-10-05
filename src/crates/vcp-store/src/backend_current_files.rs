// SPDX-License-Identifier: Apache-2.0
//! Full native cold replay, with the legacy archival prefix released at the
//! authenticated migration boundary. Persisted roots never replace replay.
use super::*;
use crate::{
    history_index::Pages,
    journal_frame::{self, ReadFrame},
    replay_base::ReplayBase,
};
use vcp_domain::Watermark;

impl Journal {
    pub(crate) async fn replay_current_all(
        &mut self,
        pages: &mut impl Pages,
        origin: &Origin,
        base: Option<&ReplayBase>,
        diagnostics: &mut crate::StoreDiagnostics,
    ) -> Result<DurableOwner> {
        let tip = latest_tip(&self.root)?;
        if let Some(tip) = &tip {
            if !matches!(tip["version"].as_u64(), Some(1 | 3))
                || self.file.metadata()?.len() < tip_end(tip)?
            {
                return Err(Error::Corruption("acknowledged journal was truncated"));
            }
        }
        let mut state = base.map(|base| base.state.clone()).unwrap_or_default();
        if state.watermark > origin.watermark() {
            return Err(Error::Corruption("history origin before base"));
        }
        let started = Instant::now();
        let checkpoints = self.read_checkpoint().and_then(|legacy| {
            self.read_current_checkpoint()
                .map(|current| (legacy, current))
        });
        diagnostics
            .checkpoint_loading
            .record(started, checkpoints.is_ok());
        let (checkpoint, current_checkpoint) = checkpoints?;
        if let Some(checkpoint) = &current_checkpoint {
            if self.file.metadata()?.len() < checkpoint.end()? {
                return Err(Error::Corruption("checkpointed journal was truncated"));
            }
        }
        if current_checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.watermark < origin.watermark())
        {
            return Err(Error::Corruption(
                "current checkpoint before history origin",
            ));
        }
        // Legacy checkpoint payloads are compared at their actual prefix cut.
        // Layout-3 checkpoints use a separate descriptor and are checked only
        // after the corresponding current owner has been semantically replayed.
        if checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.watermark > origin.watermark())
        {
            return Err(Error::Corruption("legacy checkpoint after history origin"));
        }
        let mut size = StateSize::measure(&state)?;
        diagnostics.state_size_full_scans = diagnostics.state_size_full_scans.saturating_add(1);
        let mut offset = 0u64;
        let mut chain = self.initial_chain.clone();
        let mut checkpoint_chain = chain.clone();
        if let Some(checkpoint) = checkpoint
            .as_ref()
            .filter(|checkpoint| checkpoint.watermark == state.watermark)
        {
            checkpoint.verify_observed(&state, &checkpoint_chain, diagnostics)?;
        }
        while state.watermark < origin.watermark() {
            let ReadFrame::Complete(frame) = journal_frame::read(&mut self.file, offset, &chain)?
            else {
                return Err(Error::Corruption("history origin legacy prefix missing"));
            };
            if frame.publication.is_some() {
                return Err(Error::Corruption("current frame before history origin"));
            }
            let commit: Commit = serde_json::from_slice(&frame.payload)?;
            state = state.into_replayed_observed(&commit, diagnostics, &mut size)?;
            origin
                .verify_original(pages, &commit, &frame.payload)
                .await?;
            if let Some(checkpoint) = checkpoint
                .as_ref()
                .filter(|checkpoint| state.watermark <= checkpoint.watermark)
            {
                let canonical = canonical_bytes(&commit)?;
                let header = journal_frame::header(&canonical, &checkpoint_chain, None)?;
                checkpoint_chain = digest_bytes(&[header, canonical].concat());
                if state.watermark == checkpoint.watermark {
                    checkpoint.verify_observed(&state, &checkpoint_chain, diagnostics)?;
                }
            }
            verify_tip(tip.as_ref(), state.watermark, &frame)?;
            diagnostics.replayed_commits = diagnostics.replayed_commits.saturating_add(1);
            diagnostics.replay_payload_bytes = diagnostics
                .replay_payload_bytes
                .saturating_add(frame.payload.len() as u64);
            offset = frame.end;
            chain = frame.chain;
        }
        let mut owner = origin.admit(pages, &state, base).await?;
        if let Some(checkpoint) = current_checkpoint
            .as_ref()
            .filter(|checkpoint| checkpoint.watermark == owner.semantic().current().watermark)
        {
            checkpoint.verify(&owner, &chain, offset, diagnostics)?;
        }
        drop(state);
        drop(checkpoint);
        loop {
            match journal_frame::read(&mut self.file, offset, &chain)? {
                ReadFrame::End => break,
                ReadFrame::Incomplete => {
                    if let Some(checkpoint) = &current_checkpoint {
                        if checkpoint.end()? > offset {
                            return Err(Error::Corruption("checkpointed journal was truncated"));
                        }
                    }
                    if tip
                        .as_ref()
                        .is_some_and(|tip| tip_end(tip).map_or(true, |end| end > offset))
                    {
                        return Err(Error::Corruption("acknowledged journal was truncated"));
                    }
                    self.quarantine_current_tail(offset)?;
                    break;
                }
                ReadFrame::Complete(frame) => {
                    owner = super::replay::replay_current_observed(
                        pages,
                        &owner,
                        &frame,
                        Some(diagnostics),
                    )
                    .await?;
                    verify_tip(tip.as_ref(), owner.semantic().current().watermark, &frame)?;
                    diagnostics.replayed_commits = diagnostics.replayed_commits.saturating_add(1);
                    diagnostics.replay_payload_bytes = diagnostics
                        .replay_payload_bytes
                        .saturating_add(frame.payload.len() as u64);
                    offset = frame.end;
                    chain = frame.chain;
                    if let Some(checkpoint) = current_checkpoint.as_ref().filter(|checkpoint| {
                        checkpoint.watermark == owner.semantic().current().watermark
                    }) {
                        checkpoint.verify(&owner, &chain, offset, diagnostics)?;
                    }
                }
            }
        }
        if let Some(tip) = &tip {
            let watermark: Watermark = serde_json::from_value(tip["watermark"].clone())?;
            if watermark > owner.semantic().current().watermark {
                return Err(Error::Corruption("journal tip missing"));
            }
        }
        if current_checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.watermark > owner.semantic().current().watermark)
        {
            return Err(Error::Corruption("current checkpoint ahead of history"));
        }
        self.chain = chain;
        self.file.seek(SeekFrom::Start(offset))?;
        Ok(owner)
    }
    fn quarantine_current_tail(&mut self, start: u64) -> Result<()> {
        self.file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::new();
        Read::by_ref(&mut self.file)
            .take((MAX_COMMIT_BYTES + journal_frame::CURRENT_HEADER + TRAILER) as u64)
            .read_to_end(&mut tail)?;
        if !tail.is_empty() {
            immutable_file(
                &self.root.join(format!(
                    "torn-tail-{}.bin",
                    vcp_domain::TransactionId::new()
                )),
                &tail,
            )?;
        }
        self.file.set_len(start)?;
        self.file.sync_all()?;
        Ok(())
    }
}
fn latest_tip(root: &Path) -> Result<Option<serde_json::Value>> {
    let mut latest = None;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with("commit-")
            && path.extension().is_some_and(|ext| ext == "tip")
            && latest.as_ref().is_none_or(|prior: &PathBuf| &path > prior)
        {
            latest = Some(path);
        }
    }
    latest
        .map(|path| Ok(serde_json::from_slice(&read_bounded(&path, 4096)?)?))
        .transpose()
}
fn tip_end(tip: &serde_json::Value) -> Result<u64> {
    tip["end"]
        .as_str()
        .and_then(|value| value.parse().ok())
        .ok_or(Error::Corruption("journal tip extent"))
}
fn verify_tip(
    tip: Option<&serde_json::Value>,
    watermark: Watermark,
    frame: &journal_frame::Frame,
) -> Result<()> {
    if let Some(tip) = tip {
        if tip["watermark"] != serde_json::to_value(watermark)? {
            return Ok(());
        }
        let expected_version = if frame.publication.is_some() { 3 } else { 1 };
        if tip["version"] != expected_version
            || tip["chain"] != frame.chain
            || tip_end(tip)? != frame.end
            || (expected_version == 3
                && tip["publication"].as_str() != frame.publication.as_deref())
        {
            return Err(Error::Corruption("acknowledged journal tip differs"));
        }
    }
    Ok(())
}
