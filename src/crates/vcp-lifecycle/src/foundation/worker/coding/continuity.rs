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
    pub fn configure_continuity(&mut self, binding: &ThreadBinding, config: Config) -> Result<()> {
        self.can_start(binding)?;
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
        let verified = projection.revalidate(revisions, &history, |id| {
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
        let before = encode(&parts)?;
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
        self.capture(&binding.scope, Channel::Evidence, &canonical_bytes(&serde_json::json!({
            "projection":projection,"summary_artifact":summary.spec.id,
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
        Ok(projected)
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
