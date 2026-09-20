// SPDX-License-Identifier: Apache-2.0
use super::super::backup_checkpoint::{Cut, Prepared};
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use vcp_store::snapshot_inputs::{Checkpoint, GitArtifacts};

impl Context {
    pub fn accept_backup_generation(
        &mut self,
        cut: Cut,
        files: vcp_memory::publication::SnapshotFiles,
        cancelled: &AtomicBool,
    ) -> Result<vcp_store::snapshot_inputs::GenerationInput> {
        let current = self.backup_cut()?;
        let manifest = files.manifest();
        let canonical: vcp_domain::search::Generation = self
            .engine
            .store()
            .state()
            .record(
                Collection::Generation,
                manifest.id.as_str(),
                &current.workspace.id,
            )?
            .decode()?;
        if current.workspace != cut.workspace
            || current.controller != cut.controller
            || current.epoch != cut.epoch
            || canonical != *manifest
            || manifest.scope != current.scope
            || manifest.deletion != current.workspace.deletion
            || manifest.authority != current.workspace.authority
            || cancelled.load(Ordering::Acquire)
        {
            return Err("backup generation authority or source changed".into());
        }
        let mut records = Vec::new();
        let mut captured = std::collections::BTreeMap::new();
        for (name, bytes) in files.files() {
            if cancelled.load(Ordering::Acquire) {
                return Err("backup generation capture cancelled".into());
            }
            let id = self.backup_artifact_for_generation(
                &current.scope,
                bytes,
                "vcp-backup-generation-component/1",
                Some(&manifest.id),
                &mut records,
            )?;
            let row = records
                .last_mut()
                .ok_or("backup component record missing")?;
            row.references = files.source_references().clone();
            row.references
                .insert(key(Collection::Generation, manifest.id.as_str()));
            captured.insert(name.clone(), id);
        }
        let inventory = captured
            .remove("inventory.json")
            .ok_or("backup inventory absent")?;
        let lexical_manifest = captured
            .remove("lexical/vcp-lexical.json")
            .ok_or("backup lexical manifest absent")?;
        let vectors = captured.remove("vectors.json");
        let lexical_files = captured
            .into_iter()
            .map(|(name, id)| {
                name.strip_prefix("lexical/")
                    .map(|name| (name.to_owned(), id))
                    .ok_or("unexpected backup generation component")
            })
            .collect::<std::result::Result<_, _>>()?;
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.engine.store().state().watermark,
            mutations: records
                .into_iter()
                .map(|record| Mutation::Put {
                    expected: None,
                    record,
                })
                .collect(),
            events: vec![],
            command: None,
        };
        if cancelled.load(Ordering::Acquire) {
            return Err("backup generation capture cancelled before admission".into());
        }
        self.runtime
            .block_on(self.engine.store_mut().transact(transaction))?;
        Ok(vcp_store::snapshot_inputs::GenerationInput {
            id: manifest.id.clone(),
            inventory,
            lexical_manifest,
            lexical_files,
            vectors,
        })
    }
    pub fn backup_cut(&self) -> Result<Cut> {
        if !self.owner_alive || self.authority_pending || self.interrupted_capture {
            return Err("backup capture is fenced by canonical recovery".into());
        }
        let workspace: Workspace = self
            .engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                self.config.workspace.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if workspace.trust != Trust::Trusted || workspace.authority != self.access.authority {
            return Err(
                "current workspace trust and authority are required for backup source capture"
                    .into(),
            );
        }
        let id = RootId::parse(workspace.id.as_str())?;
        for tool in ["vcp_read", "vcp_list", "vcp_search", "vcp_exec"] {
            self.tool_read_access(&id, tool)?;
        }
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        Ok(Cut {
            root: self.tool_root()?,
            workspace,
            scope: task.scope,
            controller: self.engine.controller().clone(),
            epoch: self.engine.owner_epoch(),
        })
    }
    pub fn accept_backup_checkpoint(
        &mut self,
        prepared: Prepared,
        cancelled: &AtomicBool,
    ) -> Result<Checkpoint> {
        let current = self.backup_cut()?;
        if cancelled.load(Ordering::Acquire)
            || current.controller != prepared.cut.controller
            || current.epoch != prepared.cut.epoch
            || current.scope != prepared.cut.scope
            || current.workspace != prepared.cut.workspace
            || current.root.identity != prepared.cut.root.identity
        {
            return Err("backup checkpoint authority or binding changed".into());
        }
        let observation = prepared.observed;
        if !observation.manifest.bounded_scan_complete
            || observation.sources.len() > 4096
            || observation
                .sources
                .iter()
                .map(|source| source.bytes.len())
                .sum::<usize>()
                > 8 * 1024 * 1024
        {
            return Err("backup checkpoint discovery incomplete or beyond bound".into());
        }
        observation
            .manifest
            .revalidate_selected(&current.root, &observation.manifest.files)?;
        let git = observation
            .git
            .ok_or("backup checkpoint requires explicit Git observation")?;
        let scope = &current.scope;
        let mut sources = std::collections::BTreeMap::new();
        let mut associations = Vec::new();
        let mut records = Vec::new();
        for source in observation.sources {
            if cancelled.load(Ordering::Acquire) {
                return Err("backup source capture cancelled".into());
            }
            let id =
                self.backup_artifact(scope, &source.bytes, "vcp-workspace-source/1", &mut records)?;
            associations.push(serde_json::json!({"artifact":id,"version":source.version}));
            if sources.insert(source.version.path, id).is_some() {
                return Err("duplicate backup source path".into());
            }
        }
        let git = GitArtifacts {
            status: self.backup_artifact(
                scope,
                &git.status,
                "vcp-workspace-git-status/1",
                &mut records,
            )?,
            index: self.backup_artifact(
                scope,
                &git.index,
                "vcp-workspace-git-index/1",
                &mut records,
            )?,
            staged_diff: self.backup_artifact(
                scope,
                &git.staged_diff,
                "vcp-workspace-git-staged/1",
                &mut records,
            )?,
            unstaged_diff: self.backup_artifact(
                scope,
                &git.unstaged_diff,
                "vcp-workspace-git-unstaged/1",
                &mut records,
            )?,
        };
        observation
            .manifest
            .revalidate_selected(&current.root, &observation.manifest.files)?;
        let links = records.iter().map(Record::key).collect();
        let payload = serde_json::json!({"manifest":observation.manifest,"sources":associations,"git_artifacts":git});
        let manifest = self.backup_artifact(
            scope,
            &canonical_bytes(&payload)?,
            "vcp-workspace-checkpoint/1",
            &mut records,
        )?;
        records
            .last_mut()
            .ok_or("backup manifest record missing")?
            .references = links;
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.engine.store().state().watermark,
            mutations: records
                .into_iter()
                .map(|record| Mutation::Put {
                    expected: None,
                    record,
                })
                .collect(),
            events: vec![vcp_protocol::event::EventInput {
                id: EventId::new(),
                workspace: scope.workspace.clone(),
                session: scope.session.clone(),
                task: Some(scope.task.clone()),
                actor: self.config.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: now(),
                kind: vcp_protocol::event::EventKind::ArtifactAttached,
                artifacts: vec![manifest.clone()],
                data: serde_json::json!({"operation":"backup_checkpoint","source_count":sources.len()}),
                metadata: None,
            }],
            command: None,
        };
        if cancelled.load(Ordering::Acquire) {
            return Err("backup checkpoint cancelled before admission".into());
        }
        self.runtime
            .block_on(self.engine.store_mut().transact(transaction))?;
        Ok(Checkpoint {
            manifest,
            sources,
            git: Some(git),
        })
    }

    fn backup_artifact(
        &mut self,
        scope: &Scope,
        bytes: &[u8],
        schema: &str,
        records: &mut Vec<Record>,
    ) -> Result<ArtifactId> {
        self.backup_artifact_for_generation(scope, bytes, schema, None, records)
    }
    fn backup_artifact_for_generation(
        &mut self,
        scope: &Scope,
        bytes: &[u8],
        schema: &str,
        generation: Option<&GenerationId>,
        records: &mut Vec<Record>,
    ) -> Result<ArtifactId> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("backup artifact exceeds qualified bound".into());
        }
        let mut spec = self.spec(scope, Channel::Evidence, schema);
        if let Some(id) = generation {
            spec.source = format!("generation:{id}");
        }
        let mut writer = self.engine.store().spool().create(spec)?;
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            writer.write_chunk(chunk)?;
        }
        let descriptor = writer.finalize()?;
        if descriptor.state != CaptureState::Complete {
            return Err("backup artifact capture incomplete".into());
        }
        let id = descriptor.spec.id.clone();
        records.push(Record::typed(
            Collection::Artifact,
            id.as_str(),
            scope.workspace.clone(),
            Revision::ZERO,
            &descriptor,
        )?);
        Ok(id)
    }
}
