// SPDX-License-Identifier: Apache-2.0
//! Memoize pure scope decoding within one immutable validation pass only.
//! Keys are the actual map keys, including malformed keys. No result survives a
//! state change, and failures are never admitted as cached facts.
use crate::{contract::Record, Result};
use std::collections::BTreeMap;
use vcp_domain::workspace::Scope;

#[derive(Default)]
pub(super) struct RecordFacts<'a> {
    scopes: BTreeMap<&'a str, Option<Scope>>,
}
impl<'a> RecordFacts<'a> {
    pub(super) fn scope(&mut self, key: &'a str, row: &Record) -> Result<Option<Scope>> {
        if let Some(scope) = self.scopes.get(key) {
            return Ok(scope.clone());
        }
        let scope = row.task_scope()?;
        self.scopes.insert(key, scope.clone());
        Ok(scope)
    }
}

#[cfg(test)]
#[path = "record_validation_facts_tests.rs"]
mod tests;
