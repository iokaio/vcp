// SPDX-License-Identifier: Apache-2.0
//! Complete artifact-reference membership, including redacted and malformed
//! provenance claims. Only mandatory semantic replay admits this index root.
use super::*;
use std::collections::BTreeSet;
use vcp_domain::ArtifactId;

fn prefix(workspace: &WorkspaceId, artifact: &ArtifactId) -> Result<String> {
    Ok(format!(
        "{}:",
        vcp_protocol::digest_bytes(&canonical_bytes(&(workspace, artifact))?)
    ))
}
fn key(workspace: &WorkspaceId, artifact: &ArtifactId, ordinal: u64) -> Result<String> {
    Ok(format!(
        "{}{}",
        prefix(workspace, artifact)?,
        ordinal_key(ordinal)
    ))
}
impl Catalog {
    pub(super) async fn append_artifact_references(
        &mut self,
        pages: &mut impl Pages,
        event: &EventEnvelope,
        ordinal: u64,
    ) -> Result<()> {
        // Match Vec::contains: an envelope appears once even when it names an
        // artifact repeatedly. Distinct envelopes are never collapsed.
        for artifact in event.event.artifacts.iter().collect::<BTreeSet<_>>() {
            self.artifacts = self
                .artifacts
                .insert(
                    pages,
                    entry(key(&event.event.workspace, artifact, ordinal)?, &ordinal)?,
                )
                .await?;
        }
        Ok(())
    }
    pub(super) async fn verify_artifact_references(
        &self,
        pages: &mut impl Pages,
        event: &EventEnvelope,
        ordinal: u64,
    ) -> Result<u64> {
        let artifacts = event.event.artifacts.iter().collect::<BTreeSet<_>>();
        for artifact in &artifacts {
            let row = self
                .artifacts
                .get(pages, &key(&event.event.workspace, artifact, ordinal)?)
                .await?
                .ok_or(Error::Corruption("history artifact reference missing"))?;
            if serde_json::from_value::<u64>(row.value)? != ordinal {
                return Err(Error::Corruption("history artifact reference ordinal"));
            }
        }
        Ok(artifacts.len() as u64)
    }

    /// Ordered matching envelopes with their global ordinals. The owner-bound
    /// index proves absence; this never scans unrelated event payloads. Empty
    /// means exhausted, while a byte-limited nonempty page may continue.
    pub(crate) async fn artifact_events(
        &self,
        pages: &mut impl Pages,
        workspace: &WorkspaceId,
        artifact: &ArtifactId,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<(u64, EventEnvelope)>> {
        self.validate()?;
        if limit == 0 || limit > PAGE_ROWS {
            return Err(Error::Limit("history artifact page"));
        }
        if let Some(after) = after {
            if after
                .checked_add(1)
                .ok_or(Error::Limit("history ordinal"))?
                > self.event_count()
            {
                return Err(Error::Conflict("history ordinal ahead of owner"));
            }
        }
        let prefix = prefix(workspace, artifact)?;
        let cursor = after.map_or_else(
            || prefix.clone(),
            |value| format!("{prefix}{}", ordinal_key(value)),
        );
        let rows = self.artifacts.page(pages, Some(&cursor), limit).await?;
        let mut result = Vec::new();
        let mut bytes = 0usize;
        let mut previous = after;
        for row in rows {
            if !row.key.starts_with(&prefix) {
                break;
            }
            let ordinal: u64 = serde_json::from_value(row.value)?;
            if row.key != format!("{prefix}{}", ordinal_key(ordinal))
                || previous.is_some_and(|before| ordinal <= before)
                || ordinal >= self.event_count()
            {
                return Err(Error::Corruption("history artifact locator"));
            }
            let item = self
                .events
                .get(pages, &ordinal_key(ordinal))
                .await?
                .ok_or(Error::Corruption("history artifact event missing"))?;
            let event_row: EventRow = serde_json::from_value(item.value)?;
            if !result.is_empty() && event_row.blob.bytes > (MAX_COMMIT_BYTES - bytes) as u64 {
                break;
            }
            let event: EventEnvelope = read_object(pages, &event_row.blob).await?;
            if event.event.id != event_row.id
                || event.watermark > self.watermark
                || &event.event.workspace != workspace
                || !event.event.artifacts.contains(artifact)
            {
                return Err(Error::Corruption("history artifact event identity"));
            }
            bytes += event_row.blob.bytes as usize;
            previous = Some(ordinal);
            result.push((ordinal, event));
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "history_catalog_artifact_tests.rs"]
mod tests;
