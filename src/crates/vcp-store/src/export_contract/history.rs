// SPDX-License-Identifier: Apache-2.0
//! Bounded live export provenance reads; current disclosure remains independent.
use super::*;
use vcp_protocol::event::EventEnvelope;

struct Pages {
    watermark: Watermark,
    end: u64,
    next: u64,
}

struct ArtifactPages {
    watermark: Watermark,
    end: u64,
    after: Option<u64>,
    workspace: WorkspaceId,
    artifact: ArtifactId,
}
impl ArtifactPages {
    async fn open<S: CanonicalStore>(store: &S, artifact: &ArtifactDescriptor) -> Result<Self> {
        Ok(Self {
            watermark: store.current().watermark,
            end: store.history_event_count().await?,
            after: None,
            workspace: artifact.spec.scope.workspace.clone(),
            artifact: artifact.spec.id.clone(),
        })
    }
    async fn next<S: CanonicalStore>(&mut self, store: &S) -> Result<Option<Vec<EventEnvelope>>> {
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("export history owner changed"));
        }
        let rows = store
            .history_artifact_events(&self.workspace, &self.artifact, self.after, 4096)
            .await?;
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("export history owner changed"));
        }
        if rows.len() > 4096 {
            return Err(Error::Corruption("export artifact page"));
        }
        if rows.is_empty() {
            return Ok(None);
        }
        let mut result = Vec::with_capacity(rows.len());
        for (ordinal, row) in rows {
            if ordinal >= self.end
                || self.after.is_some_and(|after| ordinal <= after)
                || row.watermark > self.watermark
                || row.event.workspace != self.workspace
                || !row.event.artifacts.contains(&self.artifact)
            {
                return Err(Error::Corruption("export artifact page identity"));
            }
            self.after = Some(ordinal);
            result.push(row);
        }
        Ok(Some(result))
    }
}
impl Pages {
    async fn open<S: CanonicalStore>(store: &S) -> Result<Self> {
        Ok(Self {
            watermark: store.current().watermark,
            end: store.history_event_count().await?,
            next: 0,
        })
    }
    async fn next<S: CanonicalStore>(&mut self, store: &S) -> Result<Option<Vec<EventEnvelope>>> {
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("export history owner changed"));
        }
        if self.next == self.end {
            return Ok(None);
        }
        let limit = (self.end - self.next).min(4096) as usize;
        let rows = store
            .history_events(self.next.checked_sub(1), limit)
            .await?;
        if rows.is_empty()
            || rows.len() > limit
            || rows.iter().any(|row| row.watermark > self.watermark)
        {
            return Err(Error::Corruption("export history page"));
        }
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("export history owner changed"));
        }
        self.next += rows.len() as u64;
        Ok(Some(rows))
    }
}

impl Sources {
    /// Only the explicitly bounded export source window is materialized. This
    /// never constructs canonical State or retains unrelated history payloads.
    pub async fn events_store<S: CanonicalStore>(&self, store: &S) -> Result<Vec<EventEnvelope>> {
        let mut pages = Pages::open(store).await?;
        let mut events = Vec::new();
        let mut remaining = MAX_BYTES;
        let mut count = 0;
        let mut serialization_error = None;
        while let Some(rows) = pages.next(store).await? {
            for row in rows.into_iter().filter(|row| self.includes_event(row)) {
                if count == MAX_EVENTS {
                    return Err(Error::Limit("export events"));
                }
                count += 1;
                if serialization_error.is_none() {
                    match bounded_canonical(&row, remaining) {
                        Ok(bytes) => {
                            remaining -= bytes.len();
                            events.push(row);
                        }
                        Err(error) => serialization_error = Some(error),
                    }
                }
            }
        }
        if let Some(error) = serialization_error {
            return Err(error);
        }
        Ok(events)
    }
    pub async fn capture_store<S: CanonicalStore>(
        store: &S,
        scope: Scope,
        task: Option<TaskId>,
        authority: AuthorityRevision,
    ) -> Result<Self> {
        let mut source = Self::capture_records(store.current(), scope, task, authority)?;
        source.events_digest = source.events_digest_store(store).await?;
        source.policy_digest = policies(store.current(), &source.scope.workspace)?;
        Ok(source)
    }
    pub async fn validate_current_store<S: CanonicalStore>(
        &self,
        store: &S,
        authority: AuthorityRevision,
        allowed: Option<&BTreeSet<TaskId>>,
    ) -> Result<()> {
        self.validate_records(store.current(), authority, allowed)?;
        if self.events_digest_store(store).await? != self.events_digest
            || policies(store.current(), &self.scope.workspace)? != self.policy_digest
        {
            return Err(Error::Access);
        }
        Ok(())
    }
    pub(super) fn includes_event(&self, row: &EventEnvelope) -> bool {
        row.watermark <= self.watermark
            && row.event.workspace == self.scope.workspace
            && row.event.session == self.scope.session
            && (row
                .event
                .task
                .as_ref()
                .is_some_and(|id| self.tasks.contains(id))
                || (self.task.is_none() && row.event.task.is_none()))
    }
    async fn events_digest_store<S: CanonicalStore>(&self, store: &S) -> Result<String> {
        let mut pages = Pages::open(store).await?;
        let mut digests = Vec::new();
        let mut remaining = MAX_BYTES;
        let mut count = 0;
        let mut serialization_error = None;
        while let Some(rows) = pages.next(store).await? {
            for row in rows.iter().filter(|row| self.includes_event(row)) {
                if count == MAX_EVENTS {
                    return Err(Error::Limit("export events"));
                }
                count += 1;
                // The archival contract first bounds event count, then hashes.
                // Retain an error, not event payloads, while proving that count.
                if serialization_error.is_none() {
                    match bounded_canonical(row, remaining) {
                        Ok(bytes) => {
                            remaining -= bytes.len();
                            digests.push(digest_bytes(&bytes));
                        }
                        Err(error) => serialization_error = Some(error),
                    }
                }
            }
        }
        if let Some(error) = serialization_error {
            return Err(error);
        }
        hash(&digests)
    }
}

pub async fn validate_read_store<S: CanonicalStore>(
    store: &S,
    authority: AuthorityRevision,
    allowed: Option<&BTreeSet<TaskId>>,
    artifact: &ArtifactDescriptor,
) -> Result<()> {
    let mut pages = ArtifactPages::open(store, artifact).await?;
    let mut has_provenance = false;
    while let Some(rows) = pages.next(store).await? {
        if rows.iter().any(|row| {
            row.event.workspace == artifact.spec.scope.workspace
                && row.event.artifacts.contains(&artifact.spec.id)
                && row.event.data.get("session_export").is_some()
        }) {
            has_provenance = true;
            break;
        }
    }
    if !reserved(&artifact.spec.schema) && !has_provenance {
        return Ok(());
    }
    acceptance_store(store, artifact)
        .await?
        .sources
        .validate_current_store(store, authority, allowed)
        .await
}

async fn acceptance_store<S: CanonicalStore>(
    store: &S,
    artifact: &ArtifactDescriptor,
) -> Result<Acceptance> {
    if artifact.spec.schema != PAYLOAD_SCHEMA && artifact.spec.schema != MANIFEST_SCHEMA {
        return Err(Error::Access);
    }
    let mut found = None;
    let mut pages = ArtifactPages::open(store, artifact).await?;
    while let Some(rows) = pages.next(store).await? {
        for event in rows.iter().filter(|row| {
            row.event.workspace == artifact.spec.scope.workspace
                && row.event.artifacts.contains(&artifact.spec.id)
        }) {
            if event.event.kind != EventKind::ArtifactAttached || event.redaction.is_some() {
                continue;
            }
            let Some(value) = event.event.data.get("session_export") else {
                continue;
            };
            let accepted: Acceptance = serde_json::from_value(value.clone())?;
            let receipt = store
                .command_receipt_by_id(&event.event.workspace, &event.event.correlation)
                .await?
                .ok_or(Error::Access)?;
            found = Some(acceptance_event(
                store.current(),
                artifact,
                event,
                accepted,
                &receipt,
                found.is_some(),
            )?);
        }
    }
    found.ok_or(Error::Access)
}
