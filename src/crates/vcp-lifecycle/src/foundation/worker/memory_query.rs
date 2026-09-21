// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::foundation::memory_query::{Selection, SendFence};
use vcp_context::manifest::{Kind, Part, Trust};
use vcp_memory::{retrieval, search_record::SourceBinding};

impl Context {
    pub fn memory_query_policy(
        &self,
    ) -> Result<(Option<String>, vcp_models::routing::RetrievalLimits)> {
        self.require_configured_routing()?;
        let policy = self.current_routing_policy()?;
        Ok((
            policy.as_ref().map(|policy| policy.id.clone()),
            policy
                .and_then(|policy| policy.retrieval_limits)
                .unwrap_or_default(),
        ))
    }
    /// Read current bounded native observations, including ignore/instruction
    /// probes. Canonical retained SourceBinding fingerprints alone are not proof
    /// that an editor has not changed the working tree since capture.
    pub fn validate_memory_bindings(
        &self,
        binding: &ThreadBinding,
        sources: &[SourceBinding],
    ) -> Result<()> {
        if sources.len() > 64 {
            return Err("memory source binding ceiling".into());
        }
        if sources.is_empty() {
            return Ok(());
        }
        let (root, current) = self.verification_observe(binding)?;
        for source in sources {
            if source.root != root.identity.root || source.fingerprint.repository != current.digest
            {
                return Err("memory source filesystem observation changed".into());
            }
            let artifact: ArtifactDescriptor = self
                .engine
                .store()
                .state()
                .record(
                    Collection::Artifact,
                    source.artifact.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if !current.manifest.files.iter().any(|file| {
                file.root == source.root
                    && file.path == source.path
                    && file.sha256 == artifact.sha256
                    && file.bytes == artifact.length
            }) {
                return Err("memory source file no longer matches retained evidence".into());
            }
        }
        Ok(())
    }
    fn validate_memory_sources(
        &self,
        binding: &ThreadBinding,
        fence: &retrieval::Fence,
    ) -> Result<()> {
        retrieval::revalidate_fence(self.engine.store(), &self.memory_access(), fence)?;
        self.validate_memory_bindings(binding, &fence.bindings)?;
        // Claims may carry an observed repository fingerprint without source
        // passages. A full observation fences those applicability assumptions.
        if fence.sources.iter().any(|s| {
            s.applicability
                .as_ref()
                .is_some_and(|a| a.fingerprint.is_some())
        }) {
            let (_, observed) = self.verification_observe(binding)?;
            for source in &fence.sources {
                if source
                    .applicability
                    .as_ref()
                    .and_then(|a| a.fingerprint.as_ref())
                    .is_some_and(|f| f.repository != observed.digest)
                {
                    return Err("memory claim filesystem applicability changed".into());
                }
            }
        }
        Ok(())
    }
    pub fn capture_memory_selection(
        &mut self,
        binding: &ThreadBinding,
        response: retrieval::Response,
        routing_policy: Option<String>,
    ) -> Result<Selection> {
        self.can_start(binding)?;
        if self.memory_query_policy()?.0 != routing_policy {
            return Err("memory query routing policy changed before capture".into());
        }
        if response.passages.is_empty() {
            return Ok(Selection {
                response,
                fence: None,
                resources: None,
            });
        }
        let source = response
            .fence
            .clone()
            .ok_or("retrieval result lacks a source fence")?;
        self.validate_memory_sources(binding, &source)?;
        let bytes = canonical_bytes(&response.passages)?;
        if bytes.len() > 65536 {
            return Err("memory evidence context ceiling".into());
        }
        let artifact = self.capture(
            &binding.scope,
            Channel::Evidence,
            &bytes,
            "memory-context/1",
        )?;
        let part = Part::captured_text(
            format!("memory:{}", artifact.spec.id),
            Kind::Evidence,
            Trust::Untrusted,
            &artifact,
            &bytes,
            false,
            100,
            format!(
                "governed retrieval {}; routing policy {}; disputed and inferred statements retain their labels",
                response.fusion,
                routing_policy.as_deref().unwrap_or("unconfigured")
            ),
        )?;
        let fence = SendFence {
            controller: self.engine.controller().clone(),
            epoch: self.engine.owner_epoch(),
            scope: binding.scope.clone(),
            source,
            part,
            routing_policy,
        };
        Ok(Selection {
            response,
            fence: Some(fence),
            resources: None,
        })
    }
    pub(super) fn validate_memory_context(
        &self,
        binding: &ThreadBinding,
        fence: &SendFence,
        sealed: &vcp_context::manifest::Sealed,
    ) -> Result<()> {
        if fence.controller != *self.engine.controller()
            || fence.epoch != self.engine.owner_epoch()
            || fence.scope != binding.scope
            || fence.routing_policy != self.memory_query_policy()?.0
            || !sealed
                .manifest
                .included
                .iter()
                .any(|part| part == &fence.part)
        {
            return Err("memory context capability does not match prepared evidence".into());
        }
        self.validate_memory_sources(binding, &fence.source)
    }
}
