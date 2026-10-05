// SPDX-License-Identifier: Apache-2.0
//! Exact proposal proof over bounded authenticated history pages.
use super::*;

pub(super) async fn read<S: CanonicalStore>(
    store: &S,
    effect: &Effect,
    masks: &[RetentionMask],
) -> Result<(Effect, Vec<ArtifactId>), QueryError> {
    let mut selected = None;
    let mut evidence_error = None;
    let read = crate::public::visit_history(store, |event| {
        if let Err(error) = observe(&mut selected, effect, masks, event) {
            evidence_error = Some(error);
            return Err(crate::public::PublicError::Unavailable);
        }
        Ok(())
    })
    .await;
    if let Some(error) = evidence_error {
        return Err(error);
    }
    read.map_err(|_| QueryError::InvalidData)?;
    selected.ok_or(QueryError::Unavailable)
}

fn observe(
    selected: &mut Option<(Effect, Vec<ArtifactId>)>,
    effect: &Effect,
    masks: &[RetentionMask],
    event: &EventEnvelope,
) -> Result<(), QueryError> {
    if event.event.kind != EventKind::EffectTransition
        || event.event.workspace != effect.scope.workspace
        || event.event.session != effect.scope.session
        || event.event.task.as_ref() != Some(&effect.scope.task)
        || event.event.data["schema_version"] != 1
    {
        return Ok(());
    }
    for fact in event.event.data["facts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|fact| {
            fact["collection"] == "effect"
                && fact["id"].as_str() == Some(effect.id.as_str())
                && fact["revision"] == "1"
        })
    {
        let original: Effect =
            serde_json::from_value(fact["value"].clone()).map_err(|_| QueryError::InvalidData)?;
        if original.id != effect.id
            || original.scope != effect.scope
            || original.operation_digest != effect.operation_digest
            || original.revision != Revision::new(1)
            || original.state != EffectState::Validated
            || original.cause != event.event.id
            || original.redaction.is_some()
            || event_hidden(event, masks)
        {
            return Err(QueryError::Unavailable);
        }
        if selected.is_some() {
            return Err(QueryError::Unavailable);
        }
        // Later rows must still be visited to reject duplicate or invalid proof.
        // Only the proposal and its bounded artifact references survive a page.
        *selected = Some((original, event.event.artifacts.clone()));
    }
    Ok(())
}
