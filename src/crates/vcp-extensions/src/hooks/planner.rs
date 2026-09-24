// SPDX-License-Identifier: Apache-2.0
use super::{
    input::{HookInput, HookLimits},
    registry::HookDefinition,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedHook {
    pub definition: HookDefinition,
    pub input: HookInput,
    pub identity: String,
}
impl PlannedHook {
    /// Reject tampering when reconstructing a durable plan.
    pub fn validate(&self, limits: HookLimits) -> Result<()> {
        self.definition.validate()?;
        self.input.validate(limits)?;
        if self.definition.event != self.input.event
            || self.identity != super::digest(&(&self.definition, &self.input))?
        {
            return Err(Error::Invalid("planned identity"));
        }
        Ok(())
    }
}
pub fn plan(
    definitions: &[HookDefinition],
    input: &HookInput,
    limits: HookLimits,
) -> Result<Vec<PlannedHook>> {
    input.validate(limits)?;
    if definitions.len() > 1024 || serde_json::to_vec(definitions)?.len() > 1_048_576 {
        return Err(Error::Limit("registry size"));
    }
    let mut registry = BTreeMap::new();
    for hook in definitions {
        hook.validate()?;
        if registry.insert(&hook.id, hook).is_some() {
            return Err(Error::Invalid("duplicate hook identity"));
        }
    }
    // Cross-event ordering has no meaningful lifecycle guarantee and is rejected.
    for hook in definitions {
        for id in hook.before.iter().chain(&hook.after) {
            if registry
                .get(id)
                .is_none_or(|other| other.event != hook.event)
            {
                return Err(Error::Invalid("unknown or cross-event ordering target"));
            }
        }
    }
    let selected: BTreeMap<_, _> = registry
        .into_iter()
        .filter(|(_, h)| h.event == input.event)
        .collect();
    if selected.len() > limits.max_fanout {
        return Err(Error::Limit("trigger fanout"));
    }
    let mut edges: BTreeMap<&String, BTreeSet<&String>> =
        selected.keys().map(|id| (*id, BTreeSet::new())).collect();
    for hook in selected.values() {
        for target in &hook.before {
            edges
                .get_mut(&hook.id)
                .ok_or(Error::Invalid("ordering"))?
                .insert(target);
        }
        for target in &hook.after {
            edges
                .get_mut(target)
                .ok_or(Error::Invalid("ordering"))?
                .insert(&hook.id);
        }
    }
    let mut emitted = BTreeSet::new();
    let mut result = Vec::new();
    while emitted.len() < selected.len() {
        let next = selected
            .values()
            .filter(|h| {
                !emitted.contains(&h.id)
                    && !edges
                        .iter()
                        .any(|(from, targets)| !emitted.contains(from) && targets.contains(&h.id))
            })
            .min_by_key(|h| (h.priority, &h.id))
            .ok_or(Error::Invalid("cyclic ordering"))?;
        emitted.insert(&next.id);
        result.push(PlannedHook {
            definition: (*next).clone(),
            input: input.clone(),
            identity: super::digest(&(next, input))?,
        });
    }
    Ok(result)
}
