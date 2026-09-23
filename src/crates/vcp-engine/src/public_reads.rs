// SPDX-License-Identifier: Apache-2.0
//! Bounded public projections. Canonical authority and scope precede every read;
//! artifact bytes are streamed through a capped range sink, never accumulated whole.
use crate::{
    query::{Query, QueryError, QueryResult},
    Access, Engine,
};
use std::io::{self, Write};
use vcp_domain::{
    accounting::Ledger,
    artifact::{ArtifactDescriptor, CaptureState},
    ids::*,
    retention::RetentionMask,
    task::Task,
    workspace::Workspace,
};
use vcp_protocol::methods::{self, Call};
use vcp_store::{
    contract::{CanonicalStore, Collection},
    Store,
};

fn id(value: &str) -> Result<methods::Id, QueryError> {
    value
        .to_owned()
        .try_into()
        .map_err(|_| QueryError::InvalidData)
}

impl<S: CanonicalStore> Engine<S> {
    pub(crate) fn read_task(
        &self,
        access: &Access,
        scope: &methods::Scope,
        task: &methods::Id,
    ) -> Result<Task, QueryError> {
        // query also validates workspace/session records and current authority.
        let task_id = TaskId::parse(task.as_str()).map_err(|_| QueryError::Unavailable)?;
        let result = self.query(access, &Query::Task { task: task_id })?;
        if scope.workspace.as_str() != access.workspace.as_str()
            || scope.session.as_str() != access.session.as_str()
        {
            return Err(QueryError::Access);
        }
        match result {
            QueryResult::Task { task, .. } => Ok(task),
            _ => Err(QueryError::InvalidData),
        }
    }

    /// Root accounting totals include child liabilities. Active reservations and
    /// unresolved liabilities remain distinct; missing ledgers never mean zero.
    /// This is one current snapshot, not an inspection cursor or per-step page.
    pub fn public_usage(
        &self,
        access: &Access,
        request: &methods::Inspect,
    ) -> Result<methods::UsageView, QueryError> {
        let task = self.read_task(access, &request.scope, &request.task)?;
        Call::UsageRead(request.clone())
            .validate()
            .map_err(|_| QueryError::Limit)?;
        if request.cursor.is_some() {
            return Err(QueryError::StaleCursor);
        }
        if request
            .target
            .as_ref()
            .is_some_and(|target| target.as_str() != task.root.as_str())
        {
            return Err(QueryError::Unavailable);
        }
        let root = self.read_task(access, &request.scope, &id(task.root.as_str())?)?;
        if root.root != root.scope.task || root.parent.is_some() {
            return Err(QueryError::InvalidData);
        }
        let ledger: Ledger = self
            .store()
            .state()
            .record(
                Collection::Ledger,
                root.scope.task.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Unavailable)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        ledger.validate().map_err(|_| QueryError::InvalidData)?;
        if ledger.scope != root.scope || ledger.currency.code() != "USD" {
            return Err(QueryError::InvalidData);
        }
        Ok(methods::UsageView {
            scope: request.scope.clone(),
            task: request.task.clone(),
            root: id(root.scope.task.as_str())?,
            currency: methods::Currency::Usd,
            cap_micros: ledger.cap.get().into(),
            settled_micros: ledger.settled.get().into(),
            reserved_micros: ledger.active.get().into(),
            unresolved_micros: ledger.unresolved.get().into(),
            overrun: ledger.overrun,
        })
    }
}

impl Engine<Store> {
    /// Bytes are always base64; at most 49,152 decoded bytes fit the public
    /// 65,536-character content bound. Offset/total_bytes count original bytes.
    /// sha256 identifies the entire canonical retained artifact, not this page.
    /// complete means both final range and complete capture; an aborted/pending
    /// prefix can reach total_bytes without claiming a complete observation.
    pub fn public_artifact(
        &self,
        access: &Access,
        request: &methods::ArtifactRead,
    ) -> Result<methods::ArtifactRange, QueryError> {
        let task = self.read_task(access, &request.scope, &request.task)?;
        Call::ArtifactRead(request.clone())
            .validate()
            .map_err(|_| QueryError::Limit)?;
        if task.redaction.is_some() {
            return Err(QueryError::Unavailable);
        }
        let state = self.store().state();
        let artifact: ArtifactDescriptor = state
            .record(
                Collection::Artifact,
                request.artifact.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Unavailable)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        artifact.validate().map_err(|_| QueryError::InvalidData)?;
        if artifact.spec.scope != task.scope
            || artifact.spec.id.as_str() != request.artifact.as_str()
            || artifact.state == CaptureState::Purged
            || artifact.spec.schema == "vcp-optimization-forecast-v1"
        {
            return Err(QueryError::Unavailable);
        }
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Access)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        vcp_store::export_contract::validate_read(state, access.authority, None, &artifact)
            .map_err(|_| QueryError::Unavailable)?;
        for row in state.records.values().filter(|row| {
            row.collection == Collection::Tombstone && row.workspace == access.workspace
        }) {
            let mask: RetentionMask = row.decode().map_err(|_| QueryError::InvalidData)?;
            mask.validate().map_err(|_| QueryError::InvalidData)?;
            if mask.workspace != access.workspace || mask.deletion > workspace.deletion {
                return Err(QueryError::InvalidData);
            }
            if mask.artifacts.contains(&artifact.spec.id) {
                return Err(QueryError::Unavailable);
            }
        }
        let offset: u64 = request
            .offset
            .as_str()
            .parse()
            .map_err(|_| QueryError::Limit)?;
        if offset > artifact.length.get() {
            return Err(QueryError::Limit);
        }
        let length = u64::from(request.length)
            .min(49_152)
            .min(artifact.length.get() - offset) as usize;
        let mut sink = RangeSink {
            position: 0,
            offset,
            length,
            bytes: Vec::with_capacity(length),
        };
        // Spool verifies canonical identity and the whole retained hash before
        // returning; corruption outside the requested range also fails closed.
        self.store()
            .spool()
            .read(&artifact, &mut sink)
            .map_err(|_| QueryError::Unavailable)?;
        if sink.bytes.len() != length {
            return Err(QueryError::InvalidData);
        }
        Ok(methods::ArtifactRange {
            artifact: request.artifact.clone(),
            offset: offset.into(),
            total_bytes: artifact.length.get().into(),
            encoding: methods::ArtifactEncoding::Base64,
            content: base64(&sink.bytes),
            complete: artifact.state == CaptureState::Complete
                && offset + length as u64 == artifact.length.get(),
            sha256: artifact.sha256,
        })
    }
}

struct RangeSink {
    position: u64,
    offset: u64,
    length: usize,
    bytes: Vec<u8>,
}
impl Write for RangeSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .position
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("artifact range overflow"))?;
        if end > self.offset && self.bytes.len() < self.length {
            let first = self
                .offset
                .saturating_sub(self.position)
                .min(bytes.len() as u64) as usize;
            let count = (self.length - self.bytes.len()).min(bytes.len() - first);
            self.bytes.extend_from_slice(&bytes[first..first + count]);
        }
        self.position = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        result.push(ALPHABET[((word >> 18) & 63) as usize] as char);
        result.push(ALPHABET[((word >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            ALPHABET[((word >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET[(word & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::{
        artifact::{ArtifactSpec, Channel},
        revision::*,
        task::Objective,
        verification::Fingerprint,
        workspace::{Binding, Scope},
    };
    use vcp_protocol::command::{Command, CommandEnvelope};
    use vcp_store::{
        artifact::ArtifactWriter,
        contract::{Mutation, Record, Transaction},
        BackendKind,
    };

    fn access() -> Access {
        Access {
            actor: ActorId::parse("owner").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        }
    }
    fn scope() -> methods::Scope {
        methods::Scope {
            workspace: id("workspace").unwrap(),
            session: id("session").unwrap(),
        }
    }
    async fn command(engine: &mut Engine<Store>, payload: Command, task: Option<TaskId>) {
        engine
            .handle(
                CommandEnvelope {
                    version: 1,
                    id: CommandId::new(),
                    workspace: access().workspace,
                    session: access().session,
                    task,
                    caller: access().actor,
                    controller: engine.controller().clone(),
                    owner_epoch: engine.owner_epoch(),
                    expected: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    payload,
                },
                &access(),
                &HostFacts::inspect(Timestamp::new(1)),
            )
            .await
            .unwrap();
    }
    async fn fixture(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
        let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
        command(
            &mut engine,
            Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/public-read-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
        )
        .await;
        for name in ["task", "other"] {
            let task = TaskId::parse(name).unwrap();
            command(
                &mut engine,
                Command::CreateTask {
                    root: task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "public read fixture".into(),
                        constraints: vec![],
                        acceptance: vec![],
                        source: EventId::new(),
                        steering: SteeringRevision::ZERO,
                    },
                    fingerprint: Fingerprint {
                        repository: "a".repeat(64),
                        buffers: "b".repeat(64),
                        environment: "c".repeat(64),
                    },
                    editing: false,
                    required_checks: vec![],
                },
                Some(task),
            )
            .await;
        }
        engine
    }
    async fn put(
        engine: &mut Engine<Store>,
        collection: Collection,
        name: &str,
        value: &impl serde::Serialize,
    ) {
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: engine.store().state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(collection, name, access().workspace, Revision::ZERO, value)
                    .unwrap(),
            }],
            events: vec![],
            command: None,
        };
        engine.store_mut().transact(transaction).await.unwrap();
    }
    async fn artifact(
        engine: &mut Engine<Store>,
        name: &str,
        bytes: &[u8],
        aborted: bool,
    ) -> ArtifactDescriptor {
        let task = TaskId::parse("task").unwrap();
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::parse(name).unwrap(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                media_type: "application/octet-stream".into(),
                schema: "fixture/1".into(),
                source: "fixture".into(),
                channel: Channel::Response,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        for chunk in bytes.chunks(65_536) {
            writer.write_chunk(chunk).unwrap();
        }
        let descriptor = if aborted {
            writer.abort().unwrap()
        } else {
            writer.finalize().unwrap()
        };
        command(
            engine,
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(task),
        )
        .await;
        descriptor
    }
    fn range(name: &str, offset: u64, length: u32) -> methods::ArtifactRead {
        methods::ArtifactRead {
            scope: scope(),
            task: id("task").unwrap(),
            artifact: id(name).unwrap(),
            offset: offset.into(),
            length,
        }
    }

    #[tokio::test]
    async fn public_usage_preserves_empty_ledger_cap_and_refuses_missing_or_foreign_data() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = fixture(temp.path(), backend).await;
            let mut request = methods::Inspect {
                scope: scope(),
                task: id("task").unwrap(),
                target: None,
                cursor: None,
                limit: 1,
            };
            assert_eq!(
                engine.public_usage(&access(), &request),
                Err(QueryError::Unavailable)
            );
            let ledger = Ledger {
                schema_version: 1,
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: TaskId::parse("task").unwrap(),
                },
                revision: Revision::ZERO,
                policy: PolicyRevision::ZERO,
                currency: "USD".to_owned().try_into().unwrap(),
                cap: Micros::new(u64::MAX),
                protected: Micros::ZERO,
                settled: Micros::ZERO,
                active: Micros::ZERO,
                unresolved: Micros::ZERO,
                allocations: Default::default(),
                daily: None,
                overrun: false,
            };
            put(&mut engine, Collection::Ledger, "task", &ledger).await;
            let watermark = engine.store().state().watermark;
            let view = engine.public_usage(&access(), &request).unwrap();
            assert_eq!(view.cap_micros.as_str(), "18446744073709551615");
            assert_eq!(view.settled_micros.as_str(), "0");
            assert_eq!(view.reserved_micros.as_str(), "0");
            assert_eq!(view.unresolved_micros.as_str(), "0");
            assert!(!view.overrun);
            request.target = Some(id("other").unwrap());
            assert_eq!(
                engine.public_usage(&access(), &request),
                Err(QueryError::Unavailable)
            );
            request.target = None;
            request.cursor = Some("invented".into());
            assert_eq!(
                engine.public_usage(&access(), &request),
                Err(QueryError::StaleCursor)
            );
            request.cursor = None;
            let mut revoked = access();
            revoked.authority = AuthorityRevision::new(1);
            assert_eq!(
                engine.public_usage(&revoked, &request),
                Err(QueryError::Access)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
            let engine =
                Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(engine.public_usage(&access(), &request).unwrap(), view);
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn public_artifact_bounds_binary_ranges_scope_and_logical_removal_on_both_stores() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temp = tempfile::tempdir().unwrap();
            let mut engine = fixture(temp.path(), backend).await;
            let bytes = vec![255; 70_000];
            let descriptor = artifact(&mut engine, "binary", &bytes, false).await;
            let first = engine
                .public_artifact(&access(), &range("binary", 0, 65_536))
                .unwrap();
            assert_eq!(first.content, "/".repeat(65_536));
            assert_eq!(first.sha256, vcp_protocol::digest_bytes(&bytes));
            assert_eq!(first.total_bytes.as_str(), "70000");
            assert!(!first.complete);
            let last = engine
                .public_artifact(&access(), &range("binary", 49_152, 65_536))
                .unwrap();
            assert_eq!(last.content.len(), 27_800);
            assert!(last.content.ends_with("/w=="));
            assert!(last.complete);
            let eof = engine
                .public_artifact(&access(), &range("binary", 70_000, 1))
                .unwrap();
            assert!(eof.content.is_empty() && eof.complete);
            assert_eq!(
                engine.public_artifact(&access(), &range("binary", u64::MAX, 1)),
                Err(QueryError::Limit)
            );
            assert_eq!(
                engine.public_artifact(&access(), &range("binary", 0, 0)),
                Err(QueryError::Limit)
            );
            let mut foreign = range("binary", 0, 8);
            foreign.task = id("other").unwrap();
            assert_eq!(
                engine.public_artifact(&access(), &foreign),
                Err(QueryError::Unavailable)
            );
            let mut denied = access();
            denied.read = false;
            denied.write = false;
            assert_eq!(
                engine.public_artifact(&denied, &range("binary", 0, 8)),
                Err(QueryError::Access)
            );
            artifact(&mut engine, "partial", b"retained prefix", true).await;
            let partial = engine
                .public_artifact(&access(), &range("partial", 0, 64))
                .unwrap();
            assert_eq!(partial.content, "cmV0YWluZWQgcHJlZml4");
            assert!(!partial.complete);
            artifact(&mut engine, "corrupt", &bytes, false).await;
            let corrupt_chunk = std::fs::read_dir(engine.store().spool().root().join("corrupt"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("00000000000000000001-")
                })
                .unwrap();
            // The requested first byte is intact: verification must also reject
            // damage to a later chunk outside this requested range.
            std::fs::write(&corrupt_chunk, [0]).unwrap();
            assert_eq!(
                engine.public_artifact(&access(), &range("corrupt", 0, 1)),
                Err(QueryError::Unavailable)
            );
            std::fs::write(&corrupt_chunk, &bytes[65_536..]).unwrap();
            let mask = RetentionMask {
                schema_version: 1,
                workspace: access().workspace,
                session: access().session,
                first: SessionSeq::ZERO,
                last: SessionSeq::ZERO,
                artifacts: vec![descriptor.spec.id.clone()],
                deletion: DeletionEpoch::ZERO,
                reason: "public exclusion".into(),
            };
            put(&mut engine, Collection::Tombstone, "mask", &mask).await;
            assert_eq!(
                engine.public_artifact(&access(), &range("binary", 0, 8)),
                Err(QueryError::Unavailable)
            );
            engine.into_store().close().await.unwrap();
            let engine =
                Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(
                engine.public_artifact(&access(), &range("binary", 0, 8)),
                Err(QueryError::Unavailable)
            );
            assert_eq!(
                engine
                    .public_artifact(&access(), &range("partial", 0, 64))
                    .unwrap(),
                partial
            );
            engine.into_store().close().await.unwrap();
        }
    }

    #[test]
    fn range_sink_and_base64_keep_partial_octets_exact() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        let mut sink = RangeSink {
            position: 0,
            offset: 2,
            length: 3,
            bytes: Vec::with_capacity(3),
        };
        sink.write_all(b"ab").unwrap();
        sink.write_all(b"cdef").unwrap();
        sink.write_all(&vec![0; 100_000]).unwrap();
        assert_eq!(sink.bytes, b"cde");
        assert_eq!(sink.bytes.capacity(), 3);
    }
}
