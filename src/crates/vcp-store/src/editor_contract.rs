// SPDX-License-Identifier: Apache-2.0
//! Reserved typed editor records; no draft content or mutable operation identity.
use crate::{Error, Result, contract::*};
use vcp_domain::{
    editor::{self, BufferState, ChangeSet, FileState},
    workspace::Scope,
};
pub(crate) fn kind(row: &Record) -> Result<bool> {
    match row.value["document_type"].as_str() {
        Some(editor::CHANGE | editor::BUFFERS) => {
            if row.collection != Collection::Projection {
                return Err(Error::Corruption("editor record collection"));
            }
            Ok(true)
        }
        Some(kind) if kind.starts_with("vcp_editor_") => Err(Error::Incompatible),
        _ => Ok(false),
    }
}
pub(crate) fn shape(row: &Record) -> Result<()> {
    let (id, workspace, revision) = if row.value["document_type"] == editor::CHANGE {
        let change: ChangeSet = row.decode()?;
        change.validate()?;
        (change.id, change.scope.workspace, change.revision)
    } else {
        let buffers: BufferState = row.decode()?;
        buffers.validate()?;
        (buffers.id, buffers.scope.workspace, buffers.revision)
    };
    if id != row.id || workspace != row.workspace || revision != row.revision {
        return Err(Error::Corruption("editor identity or revision"));
    }
    Ok(())
}
pub(crate) fn scope(row: &Record) -> Result<Scope> {
    if row.value["document_type"] == editor::CHANGE {
        Ok(row.decode::<ChangeSet>()?.scope)
    } else {
        Ok(row.decode::<BufferState>()?.scope)
    }
}
pub(crate) fn references(row: &Record) -> Result<std::collections::BTreeSet<String>> {
    let mut refs =
        std::collections::BTreeSet::from([key(Collection::Task, scope(row)?.task.as_str())]);
    if row.value["document_type"] == editor::CHANGE {
        for file in row.decode::<ChangeSet>()?.files {
            refs.insert(key(Collection::Effect, file.effect.as_str()));
        }
    }
    Ok(refs)
}
pub(crate) fn transition(before: &Record, after: &Record) -> Result<()> {
    if kind(before)? != kind(after)? {
        return Err(Error::Conflict("editor record type changed"));
    }
    if !kind(before)? {
        return Ok(());
    }
    if before.value["document_type"] != after.value["document_type"]
        || scope(before)? != scope(after)?
    {
        return Err(Error::Conflict("editor scope changed"));
    }
    if before.value["document_type"] == editor::BUFFERS {
        return Ok(());
    }
    let a: ChangeSet = before.decode()?;
    let b: ChangeSet = after.decode()?;
    let mut identity = b.clone();
    identity.revision = a.revision;
    identity.files = a.files.clone();
    if identity != a || a.files.len() != b.files.len() {
        return Err(Error::Conflict("editor preparation changed"));
    }
    let mut changed = 0;
    let mut only_retirement = true;
    for (old, new) in a.files.iter().zip(&b.files) {
        let mut template = new.clone();
        template.state = old.state;
        template.execution = old.execution.clone();
        template.observed = old.observed.clone();
        if template != *old {
            return Err(Error::Conflict("editor immutable file changed"));
        }
        if old == new {
            continue;
        }
        changed += 1;
        if old.state == FileState::Prepared
            && new.state == FileState::Rejected
            && new.execution.is_some()
        {
            return Err(Error::Conflict("editor retirement cannot invent execution"));
        }
        only_retirement &= old.state == FileState::Prepared
            && new.state == FileState::Rejected
            && new.execution.is_none();
        if !matches!(
            (old.state, new.state),
            (FileState::Prepared, FileState::Dispatched)
                | (FileState::Prepared, FileState::Rejected)
                | (
                    FileState::Dispatched,
                    FileState::Applied | FileState::Rejected | FileState::Unknown
                )
                | (
                    FileState::Unknown,
                    FileState::Applied | FileState::Rejected | FileState::Unknown
                )
        ) || old.execution.is_some() && old.execution != new.execution
        {
            return Err(Error::Conflict("editor receipt transition"));
        }
    }
    if changed == 0 || (changed != 1 && !only_retirement) {
        return Err(Error::Conflict("editor per-file transition"));
    }
    Ok(())
}
