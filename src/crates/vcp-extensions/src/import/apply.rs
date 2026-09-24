// SPDX-License-Identifier: Apache-2.0
use super::{
    normalize::{effective, Context},
    preview::{Field, Preview},
    Error, Preferences, Result,
};
use std::collections::BTreeSet;
pub fn apply(
    preview: &Preview,
    selected: &BTreeSet<String>,
    context: &Context,
    current: &Preferences,
) -> Result<Preferences> {
    preview.validate()?;
    if preview.context_digest != super::digest(context)?
        || preview.preferences_digest != super::digest(current)?
    {
        return Err(Error::Stale);
    }
    let effective = effective(context, current)?;
    let mut output = current.clone();
    let mut seen = BTreeSet::new();
    for change in &preview.changes {
        if !seen.insert(&change.id) {
            return Err(Error::Invalid("duplicate change"));
        }
        if !selected.contains(&change.id) {
            continue;
        }
        let old = effective.get(&change.server).ok_or(Error::Stale)?;
        if old != &change.old {
            return Err(Error::Stale);
        }
        change.new.validate()?;
        let entry = output.servers.entry(change.server.clone()).or_default();
        match change.field {
            Field::AllowedTools => {
                let new = change
                    .new
                    .allowed_tools
                    .as_ref()
                    .ok_or(Error::Invalid("missing tools"))?;
                if !new.is_subset(
                    old.allowed_tools
                        .as_ref()
                        .ok_or(Error::Invalid("missing base tools"))?,
                ) || change.new.timeout_ms != old.timeout_ms
                {
                    return Err(Error::Invalid("widened restriction"));
                }
                entry.allowed_tools = Some(new.clone());
            }
            Field::TimeoutMs => {
                let new = change
                    .new
                    .timeout_ms
                    .ok_or(Error::Invalid("missing timeout"))?;
                if new
                    > old
                        .timeout_ms
                        .ok_or(Error::Invalid("missing base timeout"))?
                    || change.new.allowed_tools != old.allowed_tools
                {
                    return Err(Error::Invalid("widened restriction"));
                }
                entry.timeout_ms = Some(new);
            }
        }
    }
    if !selected.is_subset(&seen.into_iter().cloned().collect()) {
        return Err(Error::Invalid("unknown selected field"));
    }
    output.validate()?;
    Ok(output)
}
/// Restored preferences remain bounded by today's base. Missing servers vanish.
pub fn rollback(previous: &Preferences, context: &Context) -> Result<Preferences> {
    Ok(Preferences {
        servers: effective(context, previous)?,
    })
}
