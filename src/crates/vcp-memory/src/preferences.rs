// SPDX-License-Identifier: Apache-2.0
//! Capture exact declared preferences during admitted ingestion, never on reads.
use crate::{
    access::{self, Access},
    extractors, Error, Result,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use vcp_domain::{artifact::*, memory::*, task::Task, workspace::Scope, *};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventEnvelope, EventInput, EventKind},
};
use vcp_store::{artifact::ArtifactWriter, contract::*, Store};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    memory_preference: Preference,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    key: String,
    value: String,
}

/// A host holding an admitted ingestion lease may retain the typed objective as
/// evidence. The immutable origin supplies bytes; no ordinary prose is parsed.
/// A retry reuses a complete unattached spool object at its deterministic ID.
pub async fn materialize(
    store: &mut Store,
    access: &Access,
    event: &EventEnvelope,
) -> Result<Option<Proposal>> {
    let workspace = access::authorize(store.state(), access, true)?;
    if event.event.workspace != workspace.id
        || event
            .event
            .task
            .as_ref()
            .is_some_and(|id| !access.allows_task(id))
    {
        return Err(Error::Access);
    }
    if !store.state().events.iter().any(|saved| saved == event) {
        return Err(Error::Invalid(
            "preference requires canonical origin".into(),
        ));
    }
    if !matches!(
        event.event.kind,
        EventKind::TaskCreated | EventKind::ObjectiveChanged
    ) {
        return Ok(None);
    }
    for row in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == workspace.id && r.collection == Collection::Tombstone)
    {
        let mask: vcp_audit::history::RetentionMask = row.decode()?;
        if mask.session == event.event.session
            && event.sequence >= mask.first
            && event.sequence <= mask.last
        {
            return Ok(None);
        }
    }
    let Some(task_id) = &event.event.task else {
        return Ok(None);
    };
    let scope = Scope {
        workspace: workspace.id.clone(),
        session: event.event.session.clone(),
        task: task_id.clone(),
    };
    let current: Task = store
        .state()
        .record(Collection::Task, task_id.as_str(), &workspace.id)?
        .decode()?;
    if current.scope != scope {
        return Err(Error::Access);
    }
    // Model-created children use the same host actor as direct root commands.
    // Until origin records distinguish direct user input, never capture a child
    // objective as user-preference evidence.
    if current.parent.is_some() || current.root != current.scope.task {
        return Ok(None);
    }
    let Some(facts) = event
        .event
        .data
        .get("facts")
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(None);
    };
    let Some(fact) = facts
        .iter()
        .find(|fact| fact["collection"] == "task" && fact["id"].as_str() == Some(task_id.as_str()))
    else {
        return Ok(None);
    };
    let task: Task = serde_json::from_value(fact["value"].clone())?;
    let Some(objective) = task.objectives.last() else {
        return Ok(None);
    };
    if objective.text.len() > 32768 {
        return Err(Error::Invalid(
            "preference objective exceeds capture limit".into(),
        ));
    }
    let Ok(input) = serde_json::from_str::<Input>(&objective.text) else {
        return Ok(None);
    };
    if !crate::repository::preference_matches(
        store.state(),
        access,
        &event.event.id,
        &input.memory_preference.key,
        &input.memory_preference.value,
    )? {
        return Ok(None);
    }
    let identity =
        extractors::output_identity(&workspace.id, &event.event.id, "explicit-preference")?;
    let id = ArtifactId::parse(digest_bytes(&canonical_bytes(&(
        "preference-evidence",
        &identity,
    ))?))?;
    let bytes = objective.text.as_bytes();
    let sha256 = digest_bytes(bytes);
    let spec = ArtifactSpec {
        id: id.clone(),
        scope: scope.clone(),
        media_type: "application/json".into(),
        schema: "memory-user-preference/1".into(),
        source: "explicit-user-objective".into(),
        channel: Channel::Evidence,
        retention: "workspace".into(),
        omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
    };
    let proposal = Proposal {
        id: ProposalId::parse(digest_bytes(&canonical_bytes(&("proposal", &identity))?))?,
        command: CommandId::parse(digest_bytes(&canonical_bytes(&("command", &identity))?))?,
        claim: ClaimId::parse(digest_bytes(&canonical_bytes(&("claim", &identity))?))?,
        scope: scope.clone(),
        actor: access.actor.clone(),
        epochs: Epochs {
            authority: workspace.authority,
            deletion: workspace.deletion,
            policy: access::policy(store.state(), &workspace.id)?,
        },
        registry_version: REGISTRY_VERSION,
        extractor: extractors::SPEC.into(),
        output_key: "explicit-preference".into(),
        origins: vec![event.event.id.clone()],
        subject: format!("preference:{}", input.memory_preference.key),
        predicate: "user_preference".into(),
        statement: format!(
            "{} = {}",
            input.memory_preference.key, input.memory_preference.value
        ),
        value: ClaimValue::UserPreference {
            key: input.memory_preference.key,
            value: input.memory_preference.value,
            explicit_origin: event.event.id.clone(),
        },
        applicability: Applicability {
            repository: workspace.binding.repository.clone(),
            worktree: workspace.binding.worktree.clone(),
            roots: vec![RootId::parse(workspace.id.as_str())?],
            paths: vec![],
            symbols: vec![],
            branch: None,
            fingerprint: None,
            conditions: BTreeMap::new(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![EvidenceRef {
            artifact: id.clone(),
            sha256: sha256.clone(),
            range: None,
            source: None,
            verification: None,
            kind: EvidenceKind::UserStatement,
        }],
        predecessor: None,
        correction_reason: None,
        retention: "workspace".into(),
    };
    proposal.validate()?;
    let existing = store
        .state()
        .records
        .get(&key(Collection::Artifact, id.as_str()));
    let descriptor = if let Some(record) = existing {
        if record.workspace != workspace.id {
            return Err(Error::Access);
        }
        let descriptor: ArtifactDescriptor = record.decode()?;
        if descriptor.spec != spec
            || descriptor.sha256 != sha256
            || !crate::proof::complete_capture(&descriptor)
        {
            return Err(Error::Conflict("preference artifact identity differs"));
        }
        vcp_audit::history::History::read_artifact(store, &access.history(), &id, std::io::sink())
            .map_err(|_| Error::Access)?;
        return Ok(Some(proposal));
    } else {
        match store.spool().inspect(&id) {
            Ok(descriptor) => {
                if descriptor.spec != spec
                    || descriptor.sha256 != sha256
                    || !crate::proof::complete_capture(&descriptor)
                {
                    return Err(Error::Conflict("unfinished or changed preference capture"));
                }
                store.spool().verify(&descriptor)?;
                descriptor
            }
            Err(vcp_store::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut writer = store.spool().create(spec)?;
                writer.write_chunk(bytes)?;
                writer.finalize()?
            }
            Err(error) => return Err(error.into()),
        }
    };
    let record = Record::typed(
        Collection::Artifact,
        id.as_str(),
        workspace.id.clone(),
        Revision::ZERO,
        &descriptor,
    )?;
    let transaction = Transaction {
        id: TransactionId::parse(digest_bytes(&canonical_bytes(&(
            "preference-capture",
            &identity,
        ))?))?,
        expected_watermark: store.state().watermark,
        mutations: vec![Mutation::Put {
            expected: None,
            record: record.clone(),
        }],
        events: vec![EventInput {
            id: EventId::parse(digest_bytes(&canonical_bytes(&(
                "preference-captured",
                &identity,
            ))?))?,
            workspace: workspace.id,
            session: scope.session,
            task: Some(scope.task),
            actor: access.actor.clone(),
            correlation: proposal.command.clone(),
            causation: Some(event.event.id.clone()),
            timestamp: event.event.timestamp,
            kind: EventKind::ArtifactAttached,
            artifacts: vec![id],
            data: serde_json::json!({"schema_version":1,"facts":[record]}),
            metadata: None,
        }],
        command: None,
    };
    store.transact(transaction).await?;
    Ok(Some(proposal))
}
