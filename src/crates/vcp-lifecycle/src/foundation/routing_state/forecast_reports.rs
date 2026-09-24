// SPDX-License-Identifier: Apache-2.0
//! Source-linked saved forecast bytes, separate from legacy snapshot reports.
use super::{
    authorize, commit, compaction_diagnostics, err, forecasts, row, OptimizationReport, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    artifact::{ArtifactDescriptor, ArtifactSpec, CaptureState, Channel, Omission},
    task::Task,
    workspace::Workspace,
    *,
};
use vcp_memory::{
    access::Access,
    retention::{recall_allowed, Target},
};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{key, CanonicalStore, Collection, Mutation, Record, Transaction},
    Store,
};

pub const SCHEMA: &str = "vcp-optimization-forecast-v1";
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TASKS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastPin {
    pub artifact: ArtifactId,
    pub digest: String,
    pub source_tasks: BTreeSet<TaskId>,
    pub source_manifest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    report: String,
    source_tasks: BTreeSet<TaskId>,
    forecast: forecasts::Report,
    compaction: compaction_diagnostics::Report,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedForecast {
    pub forecast: forecasts::Report,
    pub compaction: compaction_diagnostics::Report,
}

fn dependencies(
    forecast: &forecasts::Report,
    compaction: &compaction_diagnostics::Report,
) -> forecasts::Report {
    let mut result = forecast.clone();
    result.references.extend(
        compaction
            .source_artifacts
            .iter()
            .map(|pin| key(Collection::Artifact, pin.artifact.as_str())),
    );
    result.references.extend(
        compaction
            .source_attempts
            .iter()
            .map(|attempt| key(Collection::Attempt, attempt.as_str())),
    );
    result
        .source_events
        .extend(compaction.source_events.iter().cloned());
    result
}

fn source_access(
    store: &Store,
    access: &Access,
    value: &forecasts::Report,
    check: &dyn Fn() -> Result<()>,
) -> Result<BTreeSet<TaskId>> {
    check()?;
    authorize(store, access, false)?;
    if store.state().events.len() > 100_000 || store.state().records.len() > 100_000 {
        return Err("saved forecast source validation exceeds bounded view".into());
    }
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if value.schema_version != 1
        || value.workspace != access.workspace
        || value.authority != access.authority
        || value.deletion != workspace.deletion
        || value.serving_qualified
        || value.references.len() > 16_384
        || value.source_events.len() > 8192
    {
        return Err("saved forecast identity, authority or deletion changed".into());
    }
    let mut tasks: BTreeSet<_> = value
        .episodes
        .iter()
        .map(|e| e.task.clone())
        .chain(value.excluded.iter().map(|e| e.task.clone()))
        .collect();
    if value
        .source_tasks
        .as_ref()
        .is_some_and(|selected| selected.iter().any(|task| !access.allows_task(task)))
    {
        return Err("saved forecast source selector access changed".into());
    }
    for reference in &value.references {
        check()?;
        if !recall_allowed(
            store.state(),
            &access.workspace,
            &Target::Record(reference.clone()),
        )
        .map_err(err)?
        {
            return Err("saved forecast source excluded by retention".into());
        }
        let record = store
            .state()
            .records
            .get(reference)
            .ok_or("saved forecast source unavailable")?;
        if record.workspace != access.workspace {
            return Err("saved forecast source workspace differs".into());
        }
        let scope = if record.collection == Collection::Artifact {
            &record.value["spec"]["scope"]
        } else {
            &record.value["scope"]
        };
        let task = scope["task"]
            .as_str()
            .and_then(|id| TaskId::parse(id).ok())
            .ok_or("saved forecast source task is unavailable")?;
        if scope["workspace"].as_str() != Some(access.workspace.as_str()) {
            return Err("saved forecast source scope differs".into());
        }
        tasks.insert(task);
    }
    let events: BTreeMap<_, _> = store
        .state()
        .events
        .iter()
        .filter(|e| value.source_events.contains(&e.event.id))
        .map(|e| (&e.event.id, e))
        .collect();
    for id in &value.source_events {
        check()?;
        let event = events.get(id).ok_or("saved forecast event unavailable")?;
        if event.event.workspace != access.workspace
            || event.redaction.is_some()
            || !recall_allowed(store.state(), &access.workspace, &Target::Event(id.clone()))
                .map_err(err)?
        {
            return Err("saved forecast event access changed".into());
        }
        let task = event
            .event
            .task
            .as_ref()
            .ok_or("saved forecast event has no bounded task scope")?;
        tasks.insert(task.clone());
    }
    if tasks.len() > MAX_TASKS || tasks.iter().any(|task| !access.allows_task(task)) {
        return Err("saved forecast requires access to every source task".into());
    }
    Ok(tasks)
}

/// Write the immutable forecast artifact and its report pin in one transaction.
/// A failed transaction leaves no published report or readable canonical pin.
pub(super) async fn save(
    store: &mut Store,
    access: &Access,
    report: OptimizationReport,
    now: Timestamp,
) -> Result<OptimizationReport> {
    save_inner(store, access, report, now, false, None).await
}
struct PublicCapture<'a> {
    command: &'a super::public_optimizer::Command,
    coverage: super::public_optimizer::Coverage,
    check: &'a super::public_optimizer::Check<'a>,
    failure: std::cell::RefCell<Option<super::public_optimizer::Error>>,
}
pub(super) async fn save_public(
    store: &mut Store,
    access: &Access,
    report: OptimizationReport,
    command: &super::public_optimizer::Command,
    coverage: super::public_optimizer::Coverage,
    now: Timestamp,
    check: &super::public_optimizer::Check<'_>,
    interrupt: bool,
) -> super::public_optimizer::PublicResult<super::public_optimizer::Commit> {
    let context = PublicCapture {
        command,
        coverage,
        check,
        failure: std::cell::RefCell::new(None),
    };
    if save_inner(store, access, report, now, interrupt, Some(&context))
        .await
        .is_err()
    {
        let failure = context.failure.into_inner();
        if failure == Some(super::public_optimizer::Error::OutcomeUnknown) {
            return Err(super::public_optimizer::Error::OutcomeUnknown);
        }
        check()?;
        return Err(failure.unwrap_or(super::public_optimizer::Error::Unavailable));
    }
    super::public_optimizer::replay(store, access, command)
        .map_err(|_| super::public_optimizer::Error::OutcomeUnknown)?
        .ok_or(super::public_optimizer::Error::OutcomeUnknown)
}

/// Inject a failure after artifact finalization but before canonical publication.
/// This exercises the actual save boundary without a global hook or provider.
#[cfg(feature = "qualification")]
pub async fn qualification_interrupt_after_spool(
    store: &mut Store,
    access: &Access,
    window: super::HistoryWindow,
    now: Timestamp,
) -> Result<OptimizationReport> {
    let report = super::report(store, access, window)?;
    save_inner(store, access, report, now, true, None).await
}

async fn save_inner(
    store: &mut Store,
    access: &Access,
    mut report: OptimizationReport,
    now: Timestamp,
    interrupt: bool,
    public: Option<&PublicCapture<'_>>,
) -> Result<OptimizationReport> {
    let check = || {
        public.map_or(Ok(()), |p| {
            (p.check)().map_err(|_| "optimizer interrupted".to_owned())
        })
    };
    check()?;
    authorize(store, access, true)?;
    report.forecast = None;
    let forecast = match forecasts::observe_with_check(store, access, report.window.clone(), &check)
    {
        Ok(forecast) => forecast,
        Err(_) => {
            report.uncertainty.push("Forecast unavailable: canonical action evidence is incomplete or exceeds analysis bounds.".into());
            return save_snapshot(store, access, report, now, public).await;
        }
    };
    check()?;
    let compaction =
        compaction_diagnostics::observe_with_check(store, access, report.window.clone(), &check)?;
    check()?;
    let combined = dependencies(&forecast, &compaction);
    let tasks = match source_access(store, access, &combined, &check) {
        Ok(tasks) => tasks,
        Err(_) => {
            report.uncertainty.push("Forecast unavailable: current source access or retention does not permit a source-linked snapshot.".into());
            return save_snapshot(store, access, report, now, public).await;
        }
    };
    let Some(first) = tasks.iter().next() else {
        return save_snapshot(store, access, report, now, public).await;
    };
    let task: Task = store
        .state()
        .record(Collection::Task, first.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let document = Document {
        schema_version: 1,
        report: report.id.clone(),
        source_tasks: tasks.clone(),
        forecast,
        compaction,
    };
    let bytes = canonical_bytes(&document).map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("saved forecast exceeds four MiB; narrow the window".into());
    }
    let spec = ArtifactSpec {
        id: ArtifactId::new(),
        scope: task.scope.clone(),
        media_type: "application/json".into(),
        schema: SCHEMA.into(),
        source: "local-optimization".into(),
        channel: Channel::Evidence,
        retention: "full-work-history".into(),
        omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
    };
    let mut writer = store.spool().create(spec).map_err(err)?;
    for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
        check()?;
        writer.write_chunk(chunk).map_err(err)?;
    }
    let descriptor = writer.finalize().map_err(err)?;
    drop(writer);
    if interrupt {
        return Err("qualification interruption after forecast spool finalization".into());
    }
    let manifest_id = format!("forecast-sources-{}", report.id);
    let sources = vcp_domain::forecast::Sources {
        document_type: vcp_domain::forecast::SOURCES.into(),
        schema_version: 1,
        id: manifest_id.clone(),
        workspace: access.workspace.clone(),
        revision: Revision::ZERO,
        report: report.id.clone(),
        artifact: descriptor.spec.id.clone(),
        artifact_digest: descriptor.sha256.clone(),
        source_tasks: tasks.clone(),
        source_records: combined.references.clone(),
        source_events: combined.source_events.clone(),
        sources_digest: digest_bytes(
            &canonical_bytes(&(&tasks, &combined.references, &combined.source_events))
                .map_err(err)?,
        ),
    };
    let mut source_refs = sources.source_records.clone();
    source_refs.extend(
        tasks
            .iter()
            .map(|task| key(Collection::Task, task.as_str())),
    );
    let manifest_record = Record {
        collection: Collection::Projection,
        id: manifest_id.clone(),
        workspace: access.workspace.clone(),
        revision: Revision::ZERO,
        value: serde_json::to_value(&sources).map_err(err)?,
        references: source_refs,
    };
    manifest_record.validate_shape().map_err(err)?;
    let pin = ForecastPin {
        artifact: descriptor.spec.id.clone(),
        digest: descriptor.sha256.clone(),
        source_tasks: tasks,
        source_manifest: manifest_id.clone(),
    };
    report.forecast = Some(pin);
    let mut artifact = Record::typed(
        Collection::Artifact,
        descriptor.spec.id.as_str(),
        access.workspace.clone(),
        Revision::ZERO,
        &descriptor,
    )
    .map_err(err)?;
    artifact
        .references
        .insert(key(Collection::Projection, &manifest_id));
    artifact
        .references
        .insert(key(Collection::Task, descriptor.spec.scope.task.as_str()));
    let mut report_record = row(access, report.id.clone(), Revision::ZERO, &report)?;
    report_record
        .references
        .insert(key(Collection::Artifact, descriptor.spec.id.as_str()));
    // Only descriptor metadata appears in generic history. Aggregate payloads
    // remain behind the forecast loader's complete source-task authorization.
    let event = EventInput {
        id: EventId::new(),
        workspace: access.workspace.clone(),
        session: task.scope.session,
        task: Some(task.scope.task),
        actor: access.actor.clone(),
        correlation: CommandId::new(),
        causation: None,
        timestamp: now,
        kind: EventKind::ArtifactAttached,
        artifacts: vec![descriptor.spec.id.clone()],
        data: serde_json::json!({"schema_version":1,"facts":[{"collection":"artifact","id":descriptor.spec.id,"revision":Revision::ZERO,"value":descriptor}]}),
        metadata: None,
    };
    let mutations = vec![
        Mutation::Put {
            record: manifest_record,
            expected: None,
        },
        Mutation::Put {
            record: artifact,
            expected: None,
        },
        Mutation::Put {
            record: report_record,
            expected: None,
        },
    ];
    if let Some(public) = public {
        super::public_optimizer::commit_report(
            store,
            access,
            public.command,
            public.coverage,
            &report,
            mutations,
            vec![event],
            now,
            public.check,
        )
        .await
        .map_err(|error| {
            public.failure.replace(Some(error));
            "public optimizer publication failed".to_owned()
        })?;
        return Ok(report);
    }
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations,
            events: vec![event],
            command: None,
        })
        .await
        .map_err(err)?;
    Ok(report)
}

async fn save_snapshot(
    store: &mut Store,
    access: &Access,
    report: OptimizationReport,
    now: Timestamp,
    public: Option<&PublicCapture<'_>>,
) -> Result<OptimizationReport> {
    let record = row(access, report.id.clone(), Revision::ZERO, &report)?;
    if let Some(public) = public {
        super::public_optimizer::commit_report(
            store,
            access,
            public.command,
            public.coverage,
            &report,
            vec![Mutation::Put {
                record,
                expected: None,
            }],
            vec![],
            now,
            public.check,
        )
        .await
        .map_err(|error| {
            public.failure.replace(Some(error));
            "public optimizer publication failed".to_owned()
        })?;
        return Ok(report);
    }
    commit(
        store,
        access,
        vec![Mutation::Put {
            record,
            expected: None,
        }],
        CommandId::new(),
        now,
    )
    .await?;
    Ok(report)
}

/// Return exact saved bytes decoded under current source access. Historical
/// reports never silently replace their forecasts with a newly fitted model.
pub fn load(
    store: &Store,
    access: &Access,
    report: &OptimizationReport,
) -> Result<Option<forecasts::Report>> {
    Ok(load_saved(store, access, report)?.map(|saved| saved.forecast))
}

pub fn load_saved(
    store: &Store,
    access: &Access,
    report: &OptimizationReport,
) -> Result<Option<SavedForecast>> {
    load_saved_with_check(store, access, report, &|| Ok(()))
}
pub fn load_saved_with_check(
    store: &Store,
    access: &Access,
    report: &OptimizationReport,
    check: &dyn Fn() -> Result<()>,
) -> Result<Option<SavedForecast>> {
    check()?;
    authorize(store, access, false)?;
    let Some(pin) = &report.forecast else {
        return Ok(None);
    };
    if pin.source_tasks.len() > MAX_TASKS
        || pin
            .source_tasks
            .iter()
            .any(|task| !access.allows_task(task))
        || report.workspace != access.workspace
        || report.authority != access.authority
    {
        return Err("saved forecast report access changed".into());
    }
    let descriptor: ArtifactDescriptor = store
        .state()
        .record(
            Collection::Artifact,
            pin.artifact.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if descriptor.spec.schema != SCHEMA
        || descriptor.state != CaptureState::Complete
        || descriptor.sha256 != pin.digest
        || descriptor.length.get() > MAX_BYTES
        || descriptor.spec.scope.workspace != access.workspace
        || !pin.source_tasks.contains(&descriptor.spec.scope.task)
        || !recall_allowed(
            store.state(),
            &access.workspace,
            &Target::Record(key(Collection::Artifact, pin.artifact.as_str())),
        )
        .map_err(err)?
    {
        return Err("saved forecast artifact unavailable or changed".into());
    }
    // Generic History intentionally refuses scoped reads of aggregate payloads.
    // This loader checks the source-task manifest before and after decoding.
    let mut bytes = Vec::new();
    store.spool().read(&descriptor, &mut bytes).map_err(err)?;
    check()?;
    if digest_bytes(&bytes) != pin.digest {
        return Err("saved forecast digest differs".into());
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(err)?;
    if document.schema_version != 1
        || document.report != report.id
        || document.source_tasks != pin.source_tasks
        || document.forecast.cutoff != report.cutoff
        || document.forecast.window != report.window
        || document.forecast.deletion != report.deletion
        || document.compaction.schema_version != 1
        || document.compaction.serving_qualified
        || document.compaction.workspace != document.forecast.workspace
        || document.compaction.authority != document.forecast.authority
        || document.compaction.deletion != document.forecast.deletion
        || document.compaction.cutoff != document.forecast.cutoff
        || document.compaction.window != document.forecast.window
        || document.compaction.source_tasks != document.forecast.source_tasks
        || source_access(
            store,
            access,
            &dependencies(&document.forecast, &document.compaction),
            check,
        )? != pin.source_tasks
    {
        return Err("saved forecast source binding differs".into());
    }
    let artifact_record = store
        .state()
        .records
        .get(&key(Collection::Artifact, pin.artifact.as_str()))
        .ok_or("saved forecast record absent")?;
    let manifest_key = key(Collection::Projection, &pin.source_manifest);
    if !recall_allowed(
        store.state(),
        &access.workspace,
        &Target::Record(manifest_key.clone()),
    )
    .map_err(err)?
    {
        return Err("saved forecast source manifest excluded".into());
    }
    let manifest_record = store
        .state()
        .records
        .get(&manifest_key)
        .ok_or("saved forecast source manifest unavailable")?;
    let manifest: vcp_domain::forecast::Sources = manifest_record.decode().map_err(err)?;
    manifest.validate().map_err(err)?;
    manifest_record.validate_shape().map_err(err)?;
    let combined = dependencies(&document.forecast, &document.compaction);
    if manifest.workspace != access.workspace
        || manifest.id != pin.source_manifest
        || manifest.report != report.id
        || manifest.artifact != pin.artifact
        || manifest.artifact_digest != pin.digest
        || manifest.source_tasks != pin.source_tasks
        || manifest.source_records != combined.references
        || manifest.source_events != combined.source_events
    {
        return Err("saved forecast source manifest binding differs".into());
    }
    let expected = BTreeSet::from([
        manifest_key,
        key(Collection::Task, descriptor.spec.scope.task.as_str()),
    ]);
    if artifact_record.references != expected {
        return Err("saved forecast retention references differ".into());
    }
    let mut identity = document.forecast.clone();
    identity.id.clear();
    if document.forecast.id
        != format!(
            "action-forecast-{}",
            digest_bytes(&canonical_bytes(&identity).map_err(err)?)
        )
    {
        return Err("saved forecast content identity differs".into());
    }
    let mut compaction_identity = document.compaction.clone();
    compaction_identity.id.clear();
    if document.compaction.id
        != format!(
            "compaction-diagnostics-{}",
            digest_bytes(&canonical_bytes(&compaction_identity).map_err(err)?)
        )
    {
        return Err("saved compaction diagnostic identity differs".into());
    }
    for source in &document.compaction.source_artifacts {
        let descriptor: ArtifactDescriptor = store
            .state()
            .record(
                Collection::Artifact,
                source.artifact.as_str(),
                &access.workspace,
            )
            .map_err(err)?
            .decode()
            .map_err(err)?;
        if descriptor.sha256 != source.digest || descriptor.state != CaptureState::Complete {
            return Err("saved compaction source identity changed".into());
        }
    }
    Ok(Some(SavedForecast {
        forecast: document.forecast,
        compaction: document.compaction,
    }))
}
