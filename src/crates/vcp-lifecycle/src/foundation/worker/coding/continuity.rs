// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_context::compaction::{self, Config};

pub(super) struct Continuity {
    config: Config,
    no_gain: Option<String>,
    observed: Option<vcp_repository::observation::Manifest>,
    facts_digest: Option<String>,
}
impl Context {
    /// Pure preflight used by the async hook adapter; no projection is published.
    pub(super) fn coding_compaction_planned(&self, binding: &ThreadBinding) -> Result<bool> {
        let Some(state) = self.coding.get(&binding.scope.task) else {
            return Ok(false);
        };
        let Some(setup) = &state.continuity else {
            return Ok(false);
        };
        let identity =
            vcp_protocol::digest_bytes(&canonical_bytes(&(&state.history, &setup.config))?);
        if setup.no_gain.as_ref() == Some(&identity) {
            return Ok(false);
        }
        Ok(compaction::compact(
            &state.history,
            &self.context_revisions(binding)?,
            &setup.config,
        )?
        .is_some())
    }
    pub fn configure_continuity(&mut self, binding: &ThreadBinding, config: Config) -> Result<()> {
        self.configure_continuity_setup(binding, config, false)
    }
    pub(in crate::foundation::worker) fn continuity_configuration(
        &self,
        binding: &ThreadBinding,
    ) -> Option<Config> {
        self.coding
            .get(&binding.scope.task)
            .and_then(|state| state.continuity.as_ref())
            .map(|setup| setup.config.clone())
    }
    pub(in crate::foundation::worker) fn configure_continuity_setup(
        &mut self,
        binding: &ThreadBinding,
        config: Config,
        held_child: bool,
    ) -> Result<()> {
        if held_child {
            self.child_held_setup_access(binding)?;
        } else {
            self.can_start(binding)?;
        }
        compaction::compact(&[], &self.context_revisions(binding)?, &config)?;
        self.continuity_facts(binding)?;
        let state = self
            .coding
            .get(&binding.scope.task)
            .ok_or("coding setup missing")?;
        if !state.calls.is_empty() || state.continuity.is_some() {
            return Err("continuity setup requires an idle unconfigured coding owner".into());
        }
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&config)?,
            "canonical-continuity-configuration/1",
        )?;
        self.coding.get_mut(&binding.scope.task).unwrap().continuity = Some(Continuity {
            config,
            no_gain: None,
            observed: None,
            facts_digest: None,
        });
        Ok(())
    }
    pub fn validate_continuity_ready(&self, binding: &ThreadBinding) -> Result<()> {
        self.validate_continuity_sources(binding)?;
        if let Some(expected) = self
            .coding
            .get(&binding.scope.task)
            .and_then(|s| s.continuity.as_ref())
            .and_then(|c| c.facts_digest.as_ref())
        {
            let (facts, _) = self.continuity_facts(binding)?;
            if &vcp_protocol::digest_bytes(&canonical_bytes(&facts)?) != expected {
                return Err(
                    "current continuity effects or accounting changed before admission".into(),
                );
            }
        }
        Ok(())
    }
    pub fn validate_continuity_sources(&self, binding: &ThreadBinding) -> Result<()> {
        if let Some(expected) = self
            .coding
            .get(&binding.scope.task)
            .and_then(|s| s.continuity.as_ref())
            .and_then(|c| c.observed.as_ref())
        {
            for source in &self.coding[&binding.scope.task].history_sources {
                self.coding_artifact(source)?;
            }
            for part in &self.coding[&binding.scope.task].history {
                self.coding_artifact(&part.artifact)?;
            }
            let (_, current) = self.continuity_facts(binding)?;
            if &current != expected {
                return Err("current continuity source state changed before dispatch".into());
            }
        }
        Ok(())
    }
    pub(super) fn compact_coding_parts(
        &mut self,
        binding: &ThreadBinding,
        mut parts: Vec<Part>,
        revisions: &Revisions,
        retention_capacity: Option<u64>,
        encode: impl Fn(&[Part]) -> Result<Vec<u8>>,
    ) -> Result<Vec<Part>> {
        let Some(setup) = self
            .coding
            .get(&binding.scope.task)
            .and_then(|s| s.continuity.as_ref())
        else {
            return Ok(parts);
        };
        let config = setup.config.clone();
        let previous_no_gain = setup.no_gain.clone();
        let history = self.coding[&binding.scope.task].history.clone();
        let (facts, observed) = self.continuity_facts(binding)?;
        let facts_digest = vcp_protocol::digest_bytes(&canonical_bytes(&facts)?);
        let facts = self.coding_part(
            &binding.scope,
            Kind::TaskState,
            ContextTrust::Observed,
            Content::Text {
                text: String::from_utf8(canonical_bytes(&facts)?)?,
            },
        )?;
        // Current objective/task/instructions already precede history. Keep
        // current source, effects, checks and spend in their own mandatory part.
        let first_history = parts
            .iter()
            .position(|p| matches!(p.kind, Kind::ToolCall | Kind::ToolResult))
            .unwrap_or(parts.len());
        parts.insert(first_history, facts);
        self.coding
            .get_mut(&binding.scope.task)
            .unwrap()
            .continuity
            .as_mut()
            .unwrap()
            .observed = Some(observed);
        self.coding
            .get_mut(&binding.scope.task)
            .unwrap()
            .continuity
            .as_mut()
            .unwrap()
            .facts_digest = Some(facts_digest);
        let identity = vcp_protocol::digest_bytes(&canonical_bytes(&(&history, &config))?);
        let before = encode(&parts)?;
        // Six recent reads can evict the first file in an ordinary seven-file
        // edit loop. Keep a bounded larger window when this exact fixed-provider
        // request fits. The configured projection remains the fallback, and its
        // no-gain memo never suppresses a fresh capacity-aware retention choice.
        if let Some(expanded) = expanded_retention(&config, retention_capacity) {
            if let Some(projection) = compaction::compact(&history, revisions, &expanded)? {
                let projected =
                    self.project_coding_parts(binding, &parts, revisions, &history, &projection)?;
                let after = encode(&projected)?;
                if expanded_fits(
                    retention_capacity,
                    before.len(),
                    after.len(),
                    config.minimum_gain_bytes,
                ) {
                    self.record_coding_projection(
                        binding,
                        &projection,
                        &projected,
                        &before,
                        &after,
                    )?;
                    return Ok(projected);
                }
            } else if retention_capacity.is_some_and(|capacity| before.len() as u64 <= capacity) {
                // Complete history already fits this larger recent window.
                // Do not cache a base-policy no-gain decision: current mandatory
                // facts can grow while the original history stays unchanged.
                return Ok(parts);
            }
        }
        if previous_no_gain.as_ref() == Some(&identity) {
            return Ok(parts);
        }
        let Some(projection) = compaction::compact(&history, revisions, &config)? else {
            self.record_continuity_no_gain(
                binding,
                identity,
                "content gain below configured minimum",
            )?;
            return Ok(parts);
        };
        let projected =
            self.project_coding_parts(binding, &parts, revisions, &history, &projection)?;
        let after = encode(&projected)?;
        let gain = before.len().saturating_sub(after.len());
        if gain < config.minimum_gain_bytes {
            self.record_continuity_no_gain(
                binding,
                identity,
                "serialized provider gain below configured minimum",
            )?;
            return Ok(parts);
        }
        self.record_coding_projection(binding, &projection, &projected, &before, &after)?;
        Ok(projected)
    }
    fn project_coding_parts(
        &mut self,
        binding: &ThreadBinding,
        parts: &[Part],
        revisions: &Revisions,
        history: &[Part],
        projection: &compaction::Projection,
    ) -> Result<Vec<Part>> {
        let verified = projection.revalidate(revisions, history, |id| {
            self.coding_artifact(id)
                .map_err(|_| vcp_context::manifest::Error::Stale)
        })?;
        let summary = self.capture(
            &binding.scope,
            Channel::Evidence,
            projection.summary.as_bytes(),
            "canonical-compaction-summary/1",
        )?;
        let summary_part = verified.captured_part(&summary)?;
        let mut projected: Vec<_> = parts
            .iter()
            .filter(|p| !matches!(p.kind, Kind::ToolCall | Kind::ToolResult))
            .cloned()
            .collect();
        projected.push(summary_part);
        projected.extend(projection.retained.clone());
        Ok(projected)
    }
    fn record_coding_projection(
        &mut self,
        binding: &ThreadBinding,
        projection: &compaction::Projection,
        projected: &[Part],
        before: &[u8],
        after: &[u8],
    ) -> Result<()> {
        let summary = projected
            .iter()
            .rev()
            .find(|part| part.kind == Kind::History)
            .ok_or("compaction summary missing")?;
        let gain = before.len().saturating_sub(after.len());
        self.capture(&binding.scope, Channel::Evidence, &canonical_bytes(&serde_json::json!({
            "projection":projection,"summary_artifact":summary.artifact,
            "before_request_sha256":vcp_protocol::digest_bytes(&before),
            "after_request_sha256":vcp_protocol::digest_bytes(&after),
            "input_estimate_before":before.len(),"input_estimate_after":after.len(),
            "gain":gain,"estimate_method":"utf8-byte-ceiling/1","estimated":true,
            "meaning":"Qualified conservative request estimate, not provider-reported token usage"
        }))?, "canonical-compaction-projection/1")?;
        self.coding
            .get_mut(&binding.scope.task)
            .unwrap()
            .continuity
            .as_mut()
            .unwrap()
            .no_gain = None;
        Ok(())
    }
    fn record_continuity_no_gain(
        &mut self,
        binding: &ThreadBinding,
        identity: String,
        reason: &str,
    ) -> Result<()> {
        self.capture(
            &binding.scope,
            Channel::Evidence,
            &canonical_bytes(&serde_json::json!({"input_identity":identity,"reason":reason}))?,
            "canonical-compaction-no-gain/1",
        )?;
        self.coding
            .get_mut(&binding.scope.task)
            .unwrap()
            .continuity
            .as_mut()
            .unwrap()
            .no_gain = Some(identity);
        Ok(())
    }
}

fn expanded_retention(config: &Config, capacity: Option<u64>) -> Option<Config> {
    (capacity.is_some() && config.keep_recent_pairs < 12).then(|| Config {
        keep_recent_pairs: 12,
        ..config.clone()
    })
}

fn expanded_fits(capacity: Option<u64>, before: usize, after: usize, minimum_gain: usize) -> bool {
    capacity.is_some_and(|capacity| after as u64 <= capacity)
        && before.saturating_sub(after) >= minimum_gain
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    #[test]
    fn extra_recent_history_requires_fixed_capacity_and_exact_gain() {
        let config = Config {
            keep_recent_pairs: 6,
            preview_bytes: 512,
            minimum_gain_bytes: 2048,
        };
        assert!(
            expanded_retention(&config, None).is_none(),
            "routed behavior is unchanged"
        );
        assert_eq!(
            expanded_retention(&config, Some(191296))
                .unwrap()
                .keep_recent_pairs,
            12
        );
        assert!(expanded_retention(
            &Config {
                keep_recent_pairs: 12,
                ..config.clone()
            },
            Some(191296)
        )
        .is_none());
        assert!(expanded_fits(Some(191296), 250000, 191296, 2048));
        assert!(!expanded_fits(Some(191296), 250000, 191297, 2048));
        assert!(!expanded_fits(None, 250000, 100000, 2048));
        assert!(!expanded_fits(Some(191296), 192000, 191296, 2048));
        assert!(!expanded_fits(Some(191296), 100000, 101000, 2048));
    }
}
