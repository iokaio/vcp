// SPDX-License-Identifier: Apache-2.0
//! Narrow typed workspace provenance for aggregate forecast artifacts.
use crate::{contract::*, Error, Result};
use std::collections::BTreeSet;
use vcp_domain::{artifact::ArtifactDescriptor, forecast, workspace::Scope};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub(crate) fn kind(row: &Record) -> bool {
    row.value["document_type"] == forecast::SOURCES
}

pub(crate) fn shape(row: &Record) -> Result<()> {
    let value: forecast::Sources = row.decode()?;
    value.validate()?;
    let mut required = value.source_records.clone();
    required.extend(
        value
            .source_tasks
            .iter()
            .map(|task| key(Collection::Task, task.as_str())),
    );
    if row.collection != Collection::Projection
        || row.workspace != value.workspace
        || row.id != value.id
        || row.revision != value.revision
        || row.references != required
        || value.sources_digest
            != digest_bytes(&canonical_bytes(&(
                &value.source_tasks,
                &value.source_records,
                &value.source_events,
            ))?)
    {
        return Err(Error::Corruption(
            "forecast manifest identity or source commitment",
        ));
    }
    Ok(())
}

pub(crate) fn validate(state: &State, row: &Record) -> Result<()> {
    if !kind(row) {
        return Ok(());
    }
    shape(row)?;
    let value: forecast::Sources = row.decode()?;
    let artifact_record = state.record(
        Collection::Artifact,
        value.artifact.as_str(),
        &row.workspace,
    )?;
    let artifact: ArtifactDescriptor = artifact_record.decode()?;
    if artifact.sha256 != value.artifact_digest
        || artifact.spec.schema != "vcp-optimization-forecast-v1"
        || !value.source_tasks.contains(&artifact.spec.scope.task)
        || artifact_record.references
            != BTreeSet::from([
                row.key(),
                key(Collection::Task, artifact.spec.scope.task.as_str()),
            ])
    {
        return Err(Error::Corruption("forecast manifest artifact binding"));
    }
    let mut observed = BTreeSet::new();
    for reference in &row.references {
        let source = state
            .records
            .get(reference)
            .ok_or(Error::Corruption("forecast source absent"))?;
        if source.workspace != row.workspace {
            return Err(Error::Access);
        }
        let scope = match source.collection {
            Collection::Artifact => source.decode::<ArtifactDescriptor>()?.spec.scope,
            Collection::Task
            | Collection::Turn
            | Collection::Effect
            | Collection::Attempt
            | Collection::Settlement
            | Collection::Verification => {
                serde_json::from_value::<Scope>(source.value["scope"].clone())?
            }
            Collection::Projection
                if matches!(
                    source.value["document_type"].as_str(),
                    Some("vcp_routing_decision_v1" | "vcp_escalation_admission_v1")
                ) =>
            {
                serde_json::from_value::<Scope>(source.value["scope"].clone())?
            }
            _ => return Err(Error::Corruption("unsupported forecast source type")),
        };
        if scope.workspace != row.workspace || !value.source_tasks.contains(&scope.task) {
            return Err(Error::Access);
        }
        observed.insert(scope.task);
    }
    let events: std::collections::BTreeMap<_, _> = state
        .events
        .iter()
        .filter(|e| value.source_events.contains(&e.event.id))
        .map(|e| (&e.event.id, e))
        .collect();
    for id in &value.source_events {
        let event = events
            .get(id)
            .ok_or(Error::Corruption("forecast source event absent"))?;
        let task = event
            .event
            .task
            .as_ref()
            .ok_or(Error::Corruption("forecast event task absent"))?;
        if event.event.workspace != row.workspace || !value.source_tasks.contains(task) {
            return Err(Error::Access);
        }
        observed.insert(task.clone());
    }
    if observed != value.source_tasks {
        return Err(Error::Corruption("forecast source task manifest differs"));
    }
    Ok(())
}
