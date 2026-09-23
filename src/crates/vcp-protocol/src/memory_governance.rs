// SPDX-License-Identifier: Apache-2.0
//! Explicit candidates, not prose extraction. Runtime requires memory/governance/1.
//! Strict untagged legacy branches preserve old request serialization/digests.
use crate::methods::{Counter, Id, MemoryDecision, Mutation, Scope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CAPABILITY: &str = "memory/governance/1";

macro_rules! dto {
    ($name:ident { $($(#[$meta:meta])* $field:ident : $type:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$meta])* pub $field: $type),* }
    };
}
macro_rules! enumeration {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }
    };
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(try_from = "String", into = "String")]
#[cfg_attr(feature = "schema", schemars(transparent))]
pub struct Digest(
    #[cfg_attr(
        feature = "schema",
        schemars(regex(pattern = "^[0-9a-f]{64}(?![\\s\\S])"))
    )]
    String,
);
impl TryFrom<String> for Digest {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err("invalid SHA-256")
        }
    }
}
impl From<Digest> for String {
    fn from(value: Digest) -> Self {
        value.0
    }
}
impl Digest {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

dto!(LegacyPropose {
    scope: Scope, mutation: Mutation, task: Id,
    #[cfg_attr(feature = "schema", schemars(length(max = 65536)))] content: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))] evidence: Vec<Id>
});
dto!(LegacyResolve {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    proposal: Id,
    decision: MemoryDecision
});

// Epochs are expected current preconditions, not caller-assigned record epochs.
dto!(Guards { policy_revision: Counter, deletion_epoch: Counter, expected_head: Option<Id> });
dto!(TypedPropose {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    submission: Id,
    guards: Guards,
    candidate: Candidate
});
dto!(TypedResolve {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    submission: Id,
    submission_revision: Counter,
    submission_digest: Digest,
    guards: Guards,
    decision: MemoryDecision,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    reason: String
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ProposeParams {
    Legacy(LegacyPropose),
    Typed(TypedPropose),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ResolveParams {
    Legacy(LegacyResolve),
    Typed(TypedResolve),
}

macro_rules! common_params {
    ($name:ident) => {
        impl $name {
            pub fn mutation(&self) -> &Mutation {
                match self {
                    Self::Legacy(p) => &p.mutation,
                    Self::Typed(p) => &p.mutation,
                }
            }
            pub fn scope(&self) -> &Scope {
                match self {
                    Self::Legacy(p) => &p.scope,
                    Self::Typed(p) => &p.scope,
                }
            }
            pub fn task(&self) -> &Id {
                match self {
                    Self::Legacy(p) => &p.task,
                    Self::Typed(p) => &p.task,
                }
            }
        }
    };
}
common_params!(ProposeParams);
common_params!(ResolveParams);
fn bounded(value: &str, max: usize) -> Result<(), &'static str> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        Err("memory text bound")
    } else {
        Ok(())
    }
}
impl ProposeParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Legacy(p) => {
                if p.evidence.is_empty() || p.evidence.len() > 128 {
                    return Err("evidence list limit");
                }
                bounded(&p.content, 65536)
            }
            Self::Typed(p) => {
                p.candidate.validate()?;
                if p.candidate.predecessor.is_some()
                    && p.candidate.predecessor != p.guards.expected_head
                {
                    return Err("correction head mismatch");
                }
                Ok(())
            }
        }
    }
}
impl ResolveParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Legacy(_) => Ok(()),
            Self::Typed(p) => {
                if p.submission_revision.as_str() != "0" {
                    return Err("immutable submission revision");
                }
                bounded(&p.reason, 4096)
            }
        }
    }
}

dto!(ReviewRead {
    scope: Scope,
    task: Id,
    submission: Id
});
dto!(ReviewView {
    scope: Scope, task: Id, submission: Id, candidate_digest: Digest,
    candidate: Candidate, disposition: ReviewDisposition,
    submission_command: Id, decision_command: Option<Id>,
    governed_proposal: Option<Id>, version: Option<Id>,
    resolution: Option<crate::memory::Resolution>, indexing: Option<Indexing>,
    guards: Guards, decision: Option<MemoryDecision>, reason: Option<String>
});

enumeration!(ReviewDisposition {
    AwaitingReview,
    Resolved
});
enumeration!(Indexing {
    Pending,
    Ready,
    Failed
});
// Command acceptance and claim disposition are intentionally separate. A
// submitted candidate has no claim version, resolution or indexing promise.
dto!(ReviewResult {
    acceptance: crate::methods::Acceptance,
    submission: Id,
    candidate_digest: Digest,
    disposition: ReviewDisposition,
    governed_proposal: Option<Id>,
    version: Option<Id>,
    resolution: Option<crate::memory::Resolution>,
    indexing: Option<Indexing>
});

dto!(Candidate {
    claim: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64)))] origins: Vec<Id>,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 1024)))] subject: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))] predicate: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 16384)))] statement: String,
    value: ClaimValue, applicability: Applicability,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))] evidence: Vec<Evidence>,
    predecessor: Option<Id>,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] correction_reason: Option<String>,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))] retention: String
});

impl Candidate {
    pub fn validate(&self) -> Result<(), &'static str> {
        bounded(&self.subject, 1024)?;
        bounded(&self.predicate, 256)?;
        bounded(&self.statement, 16384)?;
        bounded(&self.retention, 256)?;
        if self.subject != self.subject.trim()
            || self.predicate != self.predicate.trim()
            || self.origins.is_empty()
            || self.origins.len() > 64
            || self.evidence.len() > 64
            || self
                .origins
                .iter()
                .map(Id::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.origins.len()
            || self
                .evidence
                .iter()
                .map(|e| e.artifact.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.evidence.len()
            || self.predecessor.is_some() != self.correction_reason.is_some()
        {
            return Err("candidate identity or collection bound");
        }
        if let Some(reason) = &self.correction_reason {
            bounded(reason, 4096)?;
        }
        // Reuse domain semantic bounds while retaining explicit public schema types.
        fn convert<T: serde::de::DeserializeOwned>(
            value: &impl Serialize,
        ) -> Result<T, &'static str> {
            serde_json::from_value(serde_json::to_value(value).map_err(|_| "candidate encoding")?)
                .map_err(|_| "candidate shape")
        }
        convert::<vcp_domain::memory::ClaimValue>(&self.value)?
            .validate()
            .map_err(|_| "claim value")?;
        convert::<vcp_domain::memory::Applicability>(&self.applicability)?
            .validate()
            .map_err(|_| "applicability")?;
        for evidence in &self.evidence {
            convert::<vcp_domain::memory::EvidenceRef>(evidence)?
                .validate()
                .map_err(|_| "evidence")?;
        }
        Ok(())
    }
}
dto!(Fingerprint {
    repository: Digest,
    buffers: Digest,
    environment: Digest
});
dto!(SourceEndpoint {
    root: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] path: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 1024)))] symbol: Option<String>,
    artifact: Id, sha256: Digest
});
enumeration!(CommandPurpose { Test, Build });
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckOutcome {
    Passed,
    Failed {
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
        reason: String,
    },
    NotRun {
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClaimValue {
    Command {
        purpose: CommandPurpose,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64)))]
        argv: Vec<String>,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
        cwd: String,
        configuration: Id,
        outcome: Option<CheckOutcome>,
        verification: Option<Id>,
    },
    ModuleRelationship {
        from: SourceEndpoint,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))]
        relation: String,
        to: SourceEndpoint,
    },
    Architecture {
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 8192)))]
        decision: String,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 8192)))]
        rationale: String,
        inference: bool,
    },
    EnvironmentConstraint {
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))]
        component: String,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
        requirement: String,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
        observed_value: Option<String>,
    },
    VerifiedFix {
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 8192)))]
        issue: String,
        patch: Id,
        verification: Id,
        before: Fingerprint,
        after: Fingerprint,
    },
    UserPreference {
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))]
        key: String,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 8192)))]
        value: String,
        explicit_origin: Id,
    },
}
dto!(Applicability {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 1024)))] repository: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 1024)))] worktree: String,
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))] roots: Vec<Id>,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))] paths: Vec<String>,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))] symbols: Vec<String>,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 1024)))] branch: Option<String>,
    fingerprint: Option<Fingerprint>,
    conditions: BTreeMap<String, String>, valid_from: Option<Counter>, valid_until: Option<Counter>
});
dto!(Range {
    start: Counter,
    end: Counter
});
enumeration!(EvidenceKind {
    Source,
    Configuration,
    ToolOutput,
    Verification,
    UserStatement,
    ModelInference,
    Patch
});
dto!(Evidence { artifact: Id, sha256: Digest, range: Option<Range>, source: Option<Fingerprint>, verification: Option<Id>, kind: EvidenceKind });

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn typed_candidate_is_explicit_and_cannot_supply_host_authority() {
        let candidate = json!({"claim":"claim","origins":["origin"],"subject":"parser",
            "predicate":"architecture","statement":"Parser isolates syntax",
            "value":{"kind":"architecture","decision":"isolate syntax","rationale":"retained source","inference":true},
            "applicability":{"repository":"repo","worktree":"main","roots":[],"paths":[],"symbols":[],
                "branch":null,"fingerprint":null,"conditions":{},"valid_from":null,"valid_until":null},
            "evidence":[{"artifact":"source","sha256":"a".repeat(64),"range":{"start":"2","end":"8"},
                "source":null,"verification":null,"kind":"source"}],
            "predecessor":null,"correction_reason":null,"retention":"workspace"});
        let proposed = json!({"scope":{"workspace":"workspace","session":"session"},
            "mutation":{"command_id":"command","expected_revision":"3","steering_revision":"2"},
            "task":"task","submission":"submission","guards":{"policy_revision":"4","deletion_epoch":"5","expected_head":null},
            "candidate":candidate});
        let decoded: ProposeParams = serde_json::from_value(proposed.clone()).unwrap();
        decoded.validate().unwrap();
        assert!(matches!(decoded, ProposeParams::Typed(_)));
        assert_eq!(serde_json::to_value(decoded).unwrap(), proposed);
        for key in [
            "actor",
            "epochs",
            "registry_version",
            "extractor",
            "scope",
            "command",
        ] {
            let mut invalid = proposed.clone();
            invalid["candidate"][key] = json!("injected");
            assert!(serde_json::from_value::<ProposeParams>(invalid).is_err());
        }
        let mut invalid = proposed;
        let mut duplicate = invalid.clone();
        duplicate["candidate"]["origins"] = json!(["origin", "origin"]);
        assert!(crate::methods::Call::decode("memory/propose", duplicate).is_err());
        let mut mismatch = invalid.clone();
        mismatch["candidate"]["predecessor"] = json!("previous");
        mismatch["candidate"]["correction_reason"] = json!("updated source");
        assert!(crate::methods::Call::decode("memory/propose", mismatch).is_err());
        invalid["candidate"]["value"]["kind"] = json!("freeform_observation");
        assert!(serde_json::from_value::<ProposeParams>(invalid).is_err());
        assert!(serde_json::from_value::<Digest>(json!(format!("{}\n", "a".repeat(64)))).is_err());
    }
    #[test]
    fn legacy_parameters_keep_exact_bytes_and_reject_mixed_authority_fields() {
        let propose = json!({"scope":{"workspace":"workspace","session":"session"},
            "mutation":{"command_id":"command","expected_revision":"0","steering_revision":"0"},
            "task":"task","content":"legacy prose","evidence":["evidence"]});
        let decoded: ProposeParams = serde_json::from_value(propose.clone()).unwrap();
        assert!(matches!(decoded, ProposeParams::Legacy(_)));
        assert_eq!(
            crate::canonical_bytes(&decoded).unwrap(),
            crate::canonical_bytes(&propose).unwrap()
        );
        let legacy: crate::methods::MemoryPropose =
            serde_json::from_value(propose.clone()).unwrap();
        assert_eq!(
            crate::canonical_bytes(&decoded).unwrap(),
            crate::canonical_bytes(&legacy).unwrap()
        );
        let mut mixed = propose;
        mixed["candidate"] = json!({});
        assert!(serde_json::from_value::<ProposeParams>(mixed).is_err());
        let resolve = json!({"scope":{"workspace":"workspace","session":"session"},
            "mutation":{"command_id":"command","expected_revision":"0","steering_revision":"0"},
            "task":"task","proposal":"proposal","decision":"accept"});
        let decoded: ResolveParams = serde_json::from_value(resolve.clone()).unwrap();
        let legacy: crate::methods::MemoryResolve =
            serde_json::from_value(resolve.clone()).unwrap();
        assert_eq!(
            crate::canonical_bytes(&decoded).unwrap(),
            crate::canonical_bytes(&legacy).unwrap()
        );
        for key in ["actor", "authority", "epochs", "controller_token"] {
            let mut injected = resolve.clone();
            injected[key] = json!("caller-choice");
            assert!(serde_json::from_value::<ResolveParams>(injected).is_err());
        }
    }
}
