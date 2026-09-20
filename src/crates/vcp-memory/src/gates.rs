// SPDX-License-Identifier: Apache-2.0
//! Pure adapter over the pinned Munarium deterministic gates. No storage backend
//! or model participates in resolution; the caller supplies one authorized view.
use munarium_core::{gates::run_gates, types as upstream};
use std::collections::BTreeSet;
use vcp_domain::{ids::*, memory::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceAvailability {
    Available,
    Unavailable,
    InvalidScope,
    Missing,
    DigestMismatch,
}

/// Host-derived observations, never deserialized from a proposal. Available
/// means current access, retained range and full spool digest were checked.
#[derive(Clone, Debug)]
pub struct EvidenceObservation {
    pub artifact: ArtifactId,
    pub status: EvidenceAvailability,
    pub verification_current: bool,
}

pub struct GovernanceContext {
    pub workspace: WorkspaceId,
    /// Current accepted versions; superseded/disputed versions cannot win canon.
    pub current: Vec<Version>,
    pub evidence: Vec<EvidenceObservation>,
    pub head: Option<ClaimVersionId>,
}

fn finding(rule: &str, severity: Severity, message: &str) -> Finding {
    Finding {
        rule: rule.into(),
        severity,
        message: message.into(),
        versions: vec![],
        evidence: vec![],
    }
}

fn reject(message: &str, rule: &str) -> Resolution {
    Resolution {
        outcome: Outcome::Rejected,
        evidence_status: EvidenceStatus::Unverified,
        findings: vec![finding(rule, Severity::Block, message)],
        conflicts: vec![],
        validated_evidence: vec![],
    }
}

fn intersects<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.is_empty() || b.is_empty() || a.iter().any(|item| b.contains(item))
}

fn path_overlaps(a: &str, b: &str) -> bool {
    // VCP's supported Windows binding cannot infer disjoint scopes from case
    // alone. Conservatively overlap aliases; preserve original source spelling.
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    a == b
        || a.strip_prefix(&b).is_some_and(|tail| tail.starts_with('/'))
        || b.strip_prefix(&a).is_some_and(|tail| tail.starts_with('/'))
}

/// Different source fingerprints represent different observations, not a later
/// timestamp silently winning an earlier claim. Empty selectors mean unrestricted.
pub fn overlaps(a: &Applicability, b: &Applicability) -> bool {
    a.repository == b.repository
        && a.worktree == b.worktree
        && intersects(&a.roots, &b.roots)
        && intersects(&a.symbols, &b.symbols)
        && (a.paths.is_empty()
            || b.paths.is_empty()
            || a.paths
                .iter()
                .any(|x| b.paths.iter().any(|y| path_overlaps(x, y))))
        && !(a.branch.is_some() && b.branch.is_some() && a.branch != b.branch)
        && !(a.fingerprint.is_some() && b.fingerprint.is_some() && a.fingerprint != b.fingerprint)
        && !a
            .conditions
            .iter()
            .any(|(key, value)| b.conditions.get(key).is_some_and(|other| other != value))
        && !a
            .valid_until
            .zip(b.valid_from)
            .is_some_and(|(end, start)| end <= start)
        && !b
            .valid_until
            .zip(a.valid_from)
            .is_some_and(|(end, start)| end <= start)
}

fn identity(proposal: &Proposal) -> Result<String, String> {
    let bytes = vcp_protocol::canonical_bytes(&(
        &proposal.subject,
        &proposal.predicate,
        proposal.value.kind(),
    ))
    .map_err(|e| e.to_string())?;
    Ok(vcp_protocol::digest_bytes(&bytes))
}

fn value(proposal: &Proposal) -> Result<String, String> {
    // Upstream values_equivalent folds case/whitespace. Typed argv, symbols and
    // values must remain exact, so adapt their canonical digest, not JSON text.
    // Rationale and evidence/provenance identities are not competing values:
    // another observation of the same preference or decision is compatible.
    let asserted = match &proposal.value {
        ClaimValue::Architecture { decision, .. } => serde_json::json!({"decision":decision}),
        ClaimValue::UserPreference { key, value, .. } => {
            serde_json::json!({"key":key,"value":value})
        }
        ClaimValue::Command {
            purpose,
            argv,
            cwd,
            configuration,
            outcome,
            ..
        } => {
            serde_json::json!({"purpose":purpose,"argv":argv,"cwd":cwd,"configuration":configuration,"outcome":outcome})
        }
        ClaimValue::VerifiedFix {
            issue,
            patch,
            before,
            after,
            ..
        } => serde_json::json!({"issue":issue,"patch":patch,"before":before,"after":after}),
        other => serde_json::to_value(other).map_err(|e| e.to_string())?,
    };
    Ok(vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&asserted).map_err(|e| e.to_string())?,
    ))
}

pub fn evaluate(proposal: &Proposal, context: &GovernanceContext) -> Result<Resolution, String> {
    if proposal.validate().is_err() {
        return Ok(reject(
            "proposal does not satisfy the versioned claim registry",
            "vcp.registry",
        ));
    }
    if proposal.scope.workspace != context.workspace {
        return Ok(reject(
            "proposal workspace is outside current authority",
            "vcp.scope",
        ));
    }
    let required_artifacts: Vec<&ArtifactId> = match &proposal.value {
        ClaimValue::Command { configuration, .. } => vec![configuration],
        ClaimValue::ModuleRelationship { from, to, .. } => vec![&from.artifact, &to.artifact],
        ClaimValue::VerifiedFix { patch, .. } => vec![patch],
        _ => vec![],
    };
    if required_artifacts
        .iter()
        .any(|artifact| !proposal.evidence.iter().any(|e| &e.artifact == *artifact))
    {
        return Ok(reject(
            "structured claim artifact is absent from its evidence references",
            "vcp.evidence-membership",
        ));
    }
    if let ClaimValue::ModuleRelationship { from, to, .. } = &proposal.value {
        for endpoint in [from, to] {
            if !proposal.applicability.roots.contains(&endpoint.root)
                || !proposal.evidence.iter().any(|e| {
                    e.artifact == endpoint.artifact
                        && e.sha256 == endpoint.sha256
                        && e.kind == EvidenceKind::Source
                })
            {
                return Ok(reject("module endpoint requires matching source evidence and explicit root applicability", "vcp.source-endpoint"));
            }
        }
    }
    let required_verification = match &proposal.value {
        ClaimValue::Command { verification, .. } => verification.as_ref(),
        ClaimValue::VerifiedFix { verification, .. } => Some(verification),
        _ => None,
    };
    if proposal.predecessor != context.head {
        return Ok(reject(
            "correction predecessor is not the current claim head",
            "vcp.predecessor",
        ));
    }
    if let Some(predecessor) = &proposal.predecessor {
        let valid = context.current.iter().any(|version| {
            version.id == *predecessor
                && version.scope.workspace == context.workspace
                && version.proposal.claim == proposal.claim
                && version.proposal.subject == proposal.subject
                && version.proposal.predicate == proposal.predicate
                && version.proposal.value.kind() == proposal.value.kind()
                && version.resolution.outcome == Outcome::Accepted
        });
        if !valid {
            return Ok(reject(
                "correction does not name an accepted version in this lineage",
                "vcp.predecessor",
            ));
        }
    }
    let mut unavailable = false;
    let mut verified = false;
    let mut validated = Vec::new();
    for reference in &proposal.evidence {
        let mut matches = context
            .evidence
            .iter()
            .filter(|row| row.artifact == reference.artifact);
        let Some(observation) = matches.next() else {
            return Ok(reject(
                "evidence identity was not resolved",
                "vcp.evidence-missing",
            ));
        };
        if matches.next().is_some() {
            return Err("ambiguous host evidence observation".into());
        }
        match observation.status {
            EvidenceAvailability::Missing
            | EvidenceAvailability::InvalidScope
            | EvidenceAvailability::DigestMismatch => {
                return Ok(reject(
                    "evidence is missing, outside current scope, or fails integrity",
                    "vcp.evidence-invalid",
                ));
            }
            EvidenceAvailability::Unavailable => unavailable = true,
            EvidenceAvailability::Available => {
                validated.push(reference.artifact.clone());
                verified |= reference.kind == EvidenceKind::Verification
                    && observation.verification_current
                    && required_verification.is_some()
                    && reference.verification.as_ref() == required_verification;
            }
        }
    }
    if proposal.evidence.is_empty() {
        return Ok(reject(
            "supported claims require retained evidence references",
            "vcp.evidence-missing",
        ));
    }
    let requires_verification = matches!(
        &proposal.value,
        ClaimValue::VerifiedFix { .. }
            | ClaimValue::Command {
                outcome: Some(_),
                ..
            }
    );
    if requires_verification && (!verified || unavailable) {
        return Ok(reject(
            "verified outcome requires available evidence and current verification",
            "vcp.verification",
        ));
    }
    let inference = matches!(
        &proposal.value,
        ClaimValue::Architecture {
            inference: true,
            ..
        }
    ) || proposal
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::ModelInference);
    let status = if unavailable {
        EvidenceStatus::Unverified
    } else if inference {
        EvidenceStatus::Inferred
    } else if requires_verification {
        EvidenceStatus::Verified
    } else {
        EvidenceStatus::Observed
    };
    let subject = identity(proposal)?;
    let proposed_value = value(proposal)?;
    let relevant: Vec<_> = context
        .current
        .iter()
        .filter(|version| {
            version.scope.workspace == context.workspace
                && version.resolution.outcome == Outcome::Accepted
                && version.proposal.subject == proposal.subject
                && version.proposal.predicate == proposal.predicate
                && version.proposal.value.kind() == proposal.value.kind()
                && overlaps(&version.proposal.applicability, &proposal.applicability)
        })
        .collect();
    if relevant.len() > 64 {
        return Ok(reject(
            "overlapping claim set exceeds the bounded governance contract",
            "vcp.governance-capacity",
        ));
    }
    let mut facts = Vec::new();
    for version in &relevant {
        facts.push(upstream::Claim {
            id: version.id.to_string(),
            version_id: context.workspace.to_string(),
            seq: version.memory_seq.get(),
            claim_type: upstream::ClaimType::Fact,
            subject: subject.clone(),
            key: "value".into(),
            value: value(&version.proposal)?,
            scope_path: Some(context.workspace.to_string()),
            status: upstream::ClaimStatus::Accepted,
            provenance: upstream::Provenance::Witnessed,
            supersedes_id: None,
            entity_id: None,
            evidence: None,
            confidence: None,
            shape_ref: None,
            origin: None,
        });
    }
    facts.sort_by(|a, b| (a.seq, &a.id).cmp(&(b.seq, &b.id)));
    let snapshot = upstream::MeshSnapshot {
        version_id: context.workspace.to_string(),
        facts,
        ..Default::default()
    };
    let proposed = upstream::ProposedClaim {
        claim_type: if proposal.predecessor.is_some() {
            upstream::ClaimType::Correction
        } else {
            upstream::ClaimType::Fact
        },
        subject,
        key: "value".into(),
        value: proposed_value.clone(),
        supersedes_id: proposal.predecessor.as_ref().map(ToString::to_string),
    };
    let mut candidate = upstream::Candidate {
        scope_path: Some(context.workspace.to_string()),
        text: proposal.statement.clone(),
        previous_texts: relevant
            .iter()
            .map(|v| v.proposal.statement.clone())
            .collect(),
        ..Default::default()
    };
    if proposal.predecessor.is_some() {
        candidate.corrections.push(proposed);
    } else {
        candidate.claims.push(proposed);
    }
    let mut upstream_findings = run_gates(&snapshot, &candidate);
    // A correction exempts its own predecessor, not other accepted lineages.
    // Check every overlapping accepted version: selecting only the latest fact
    // would make conflict preservation depend on incidental snapshot ordering.
    for fact in &snapshot.facts {
        if proposal
            .predecessor
            .as_ref()
            .is_some_and(|id| id.as_str() == fact.id)
        {
            continue;
        }
        let mut plain = candidate.clone();
        plain.text.clear();
        plain.previous_texts.clear();
        let mut claim = plain
            .claims
            .pop()
            .or_else(|| plain.corrections.pop())
            .ok_or("missing adapted claim")?;
        claim.claim_type = upstream::ClaimType::Fact;
        claim.supersedes_id = None;
        plain.claims = vec![claim];
        plain.corrections.clear();
        upstream_findings.extend(run_gates(
            &upstream::MeshSnapshot {
                facts: vec![fact.clone()],
                ..Default::default()
            },
            &plain,
        ));
    }
    let mut findings = Vec::new();
    let mut conflicts = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for upstream in upstream_findings {
        let version = upstream
            .detail
            .as_ref()
            .and_then(|detail| detail.get("canon_claim_id"))
            .and_then(|id| id.as_str())
            .map(ClaimVersionId::parse)
            .transpose()
            .map_err(|e| e.to_string())?;
        if !seen.insert((
            upstream.rule_id.clone(),
            version.clone(),
            upstream.message.clone(),
        )) {
            continue;
        }
        if upstream.severity == upstream::Severity::Block {
            if let Some(id) = &version {
                conflicts.insert(id.clone());
            }
        }
        findings.push(Finding {
            rule: upstream.rule_id,
            severity: match upstream.severity {
                upstream::Severity::Info => Severity::Info,
                upstream::Severity::Warn => Severity::Warning,
                upstream::Severity::Block => Severity::Block,
            },
            message: upstream.message,
            versions: version.into_iter().collect(),
            evidence: vec![],
        });
    }
    if unavailable {
        findings.push(finding(
            "vcp.evidence-unavailable",
            Severity::Warning,
            "retained evidence content is unavailable; this claim is not verified",
        ));
    }
    if inference {
        findings.push(finding(
            "vcp.inference",
            Severity::Info,
            "inferred claim; automatic acceptance is not verification",
        ));
    }
    validated.sort();
    validated.dedup();
    if findings.len() > MAX_FINDINGS {
        return Ok(reject(
            "gate findings exceed the bounded governance contract",
            "vcp.governance-capacity",
        ));
    }
    Ok(Resolution {
        outcome: if findings.iter().any(|f| f.severity == Severity::Block) {
            Outcome::Disputed
        } else {
            Outcome::Accepted
        },
        evidence_status: status,
        findings,
        conflicts: conflicts.into_iter().collect(),
        validated_evidence: validated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::{revision::*, verification::Fingerprint, workspace::Scope};

    fn proposal() -> Proposal {
        Proposal {
            id: ProposalId::new(),
            command: CommandId::new(),
            claim: ClaimId::new(),
            scope: Scope {
                workspace: WorkspaceId::new(),
                session: SessionId::new(),
                task: TaskId::new(),
            },
            actor: ActorId::new(),
            epochs: Epochs {
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
                policy: PolicyRevision::ZERO,
            },
            registry_version: REGISTRY_VERSION,
            extractor: "fixture/1".into(),
            output_key: "architecture".into(),
            origins: vec![EventId::new()],
            subject: "module".into(),
            predicate: "responsibility".into(),
            statement: "Owns local persistence".into(),
            value: ClaimValue::Architecture {
                decision: "local persistence".into(),
                rationale: "observed source".into(),
                inference: true,
            },
            applicability: Applicability {
                repository: "a".repeat(64),
                worktree: "b".repeat(64),
                roots: vec![],
                paths: vec![],
                symbols: vec![],
                branch: None,
                fingerprint: None,
                conditions: Default::default(),
                valid_from: None,
                valid_until: None,
            },
            evidence: vec![EvidenceRef {
                artifact: ArtifactId::new(),
                sha256: "c".repeat(64),
                range: None,
                source: None,
                verification: None,
                kind: EvidenceKind::Source,
            }],
            predecessor: None,
            correction_reason: None,
            retention: "history".into(),
        }
    }

    fn context(proposal: &Proposal) -> GovernanceContext {
        GovernanceContext {
            workspace: proposal.scope.workspace.clone(),
            current: vec![],
            head: None,
            evidence: proposal
                .evidence
                .iter()
                .map(|e| EvidenceObservation {
                    artifact: e.artifact.clone(),
                    status: EvidenceAvailability::Available,
                    verification_current: false,
                })
                .collect(),
        }
    }

    fn version(proposal: &Proposal, context: &GovernanceContext) -> Version {
        Version {
            document_type: DocumentType::Version,
            schema_version: 1,
            id: ClaimVersionId::new(),
            scope: proposal.scope.clone(),
            revision: Revision::ZERO,
            proposal: proposal.clone(),
            memory_seq: MemorySeq::new(1),
            canonical_watermark: Watermark::new(1),
            recorded_at: Timestamp::new(10),
            resolution: evaluate(proposal, context).unwrap(),
        }
    }

    #[test]
    fn automatic_inference_conflict_and_explicit_correction_preserve_original() {
        let original = proposal();
        original.validate().unwrap();
        let mut context = context(&original);
        let accepted = version(&original, &context);
        assert_eq!(accepted.resolution.outcome, Outcome::Accepted);
        assert_eq!(
            accepted.resolution.evidence_status,
            EvidenceStatus::Inferred
        );
        context.current.push(accepted.clone());
        let mut conflict = original.clone();
        conflict.id = ProposalId::new();
        conflict.claim = ClaimId::new();
        conflict.command = CommandId::new();
        if let ClaimValue::Architecture { decision, .. } = &mut conflict.value {
            *decision = "remote persistence".into();
        }
        let disputed = evaluate(&conflict, &context).unwrap();
        assert_eq!(disputed.outcome, Outcome::Disputed);
        assert_eq!(disputed.conflicts, vec![accepted.id.clone()]);
        assert!(disputed
            .findings
            .iter()
            .any(|f| f.rule == munarium_core::gates::RULE_LEDGER));
        let mut corrected = conflict;
        corrected.claim = original.claim.clone();
        corrected.predecessor = Some(accepted.id.clone());
        corrected.correction_reason = Some("explicit correction of earlier inference".into());
        context.head = Some(accepted.id.clone());
        assert_eq!(
            evaluate(&corrected, &context).unwrap().outcome,
            Outcome::Accepted
        );
        assert_eq!(context.current[0], accepted);
        context.head = Some(ClaimVersionId::new());
        assert_eq!(
            evaluate(&corrected, &context).unwrap().outcome,
            Outcome::Rejected
        );
    }

    #[test]
    fn evidence_failures_reject_but_unavailable_inference_is_labelled() {
        let proposal = proposal();
        let mut context = context(&proposal);
        for status in [
            EvidenceAvailability::Missing,
            EvidenceAvailability::InvalidScope,
            EvidenceAvailability::DigestMismatch,
        ] {
            context.evidence[0].status = status;
            assert_eq!(
                evaluate(&proposal, &context).unwrap().outcome,
                Outcome::Rejected
            );
        }
        context.evidence[0].status = EvidenceAvailability::Unavailable;
        let resolution = evaluate(&proposal, &context).unwrap();
        assert_eq!(resolution.outcome, Outcome::Accepted);
        assert_eq!(resolution.evidence_status, EvidenceStatus::Unverified);
        assert!(resolution.validated_evidence.is_empty());
        context.evidence.clear();
        assert_eq!(
            evaluate(&proposal, &context).unwrap().outcome,
            Outcome::Rejected
        );
    }

    #[test]
    fn verified_outcome_requires_its_own_current_verification() {
        let mut proposal = proposal();
        let required = VerificationId::new();
        proposal.value = ClaimValue::Command {
            purpose: CommandPurpose::Test,
            argv: vec!["cargo".into(), "test".into()],
            cwd: "src".into(),
            configuration: proposal.evidence[0].artifact.clone(),
            outcome: Some(vcp_domain::verification::CheckOutcome::Passed),
            verification: Some(required.clone()),
        };
        proposal.evidence[0].kind = EvidenceKind::Configuration;
        proposal.evidence.push(EvidenceRef {
            artifact: ArtifactId::new(),
            sha256: "d".repeat(64),
            range: None,
            source: None,
            verification: Some(VerificationId::new()),
            kind: EvidenceKind::Verification,
        });
        let mut context = context(&proposal);
        context.evidence[1].verification_current = true;
        assert_eq!(
            evaluate(&proposal, &context).unwrap().outcome,
            Outcome::Rejected
        );
        proposal.evidence[1].verification = Some(required);
        let verified = evaluate(&proposal, &context).unwrap();
        assert_eq!(verified.outcome, Outcome::Accepted);
        assert_eq!(verified.evidence_status, EvidenceStatus::Verified);
        context.evidence[1].verification_current = false;
        assert_eq!(
            evaluate(&proposal, &context).unwrap().outcome,
            Outcome::Rejected
        );
    }

    #[test]
    fn exact_typed_values_and_every_overlapping_lineage_are_checked() {
        let original = proposal();
        let mut context = context(&original);
        let accepted = version(&original, &context);
        context.current.push(accepted.clone());
        let mut changed = original.clone();
        if let ClaimValue::Architecture { rationale, .. } = &mut changed.value {
            *rationale = "additional compatible evidence".into();
        }
        assert_eq!(
            evaluate(&changed, &context).unwrap().outcome,
            Outcome::Accepted
        );
        if let ClaimValue::Architecture { decision, .. } = &mut changed.value {
            *decision = decision.to_uppercase();
        }
        assert_eq!(
            evaluate(&changed, &context).unwrap().outcome,
            Outcome::Disputed
        );
        let mut another = accepted.clone();
        another.id = ClaimVersionId::new();
        another.proposal.claim = ClaimId::new();
        context.current.push(another.clone());
        changed.predecessor = Some(accepted.id.clone());
        changed.correction_reason = Some("change one lineage only".into());
        context.head = Some(accepted.id);
        let resolution = evaluate(&changed, &context).unwrap();
        assert_eq!(resolution.outcome, Outcome::Disputed);
        assert_eq!(resolution.conflicts, vec![another.id]);
    }

    #[test]
    fn different_source_observations_and_exclusive_conditions_do_not_conflict() {
        let mut first = proposal();
        first.applicability.fingerprint = Some(Fingerprint {
            repository: "d".repeat(64),
            buffers: "e".repeat(64),
            environment: "f".repeat(64),
        });
        let mut context = context(&first);
        context.current.push(version(&first, &context));
        let mut later = first.clone();
        later.applicability.fingerprint.as_mut().unwrap().repository = "0".repeat(64);
        if let ClaimValue::Architecture { decision, .. } = &mut later.value {
            *decision = "different observation".into();
        }
        assert_eq!(
            evaluate(&later, &context).unwrap().outcome,
            Outcome::Accepted
        );
        let mut a = first.applicability.clone();
        let mut b = a.clone();
        a.conditions.insert("platform".into(), "windows".into());
        b.conditions.insert("platform".into(), "linux".into());
        assert!(!overlaps(&a, &b));
        a.conditions.clear();
        b.conditions.clear();
        a.paths = vec!["src/core".into()];
        b.paths = vec!["src/core/file.rs".into()];
        assert!(overlaps(&a, &b));
        b.paths = vec!["src/core-other".into()];
        assert!(!overlaps(&a, &b));
    }

    /// Conformance cases adapted from the pinned Apache-2.0 upstream
    /// munarium-core/src/gates.rs tests. Rule IDs/severity/order stay upstream;
    /// VCP's stricter predecessor rejection and exact typed values are separate.
    #[test]
    fn retained_upstream_anchor_dedup_orphan_meta_and_similarity_conformance() {
        use munarium_core::gates::{RULE_ANCHOR, RULE_META, RULE_ORPHAN, RULE_SIMILARITY};
        let mut snapshot = upstream::MeshSnapshot::default();
        snapshot.facts.push(upstream::Claim {
            id: "c1".into(),
            version_id: "v1".into(),
            seq: 1,
            claim_type: upstream::ClaimType::Fact,
            subject: "hero".into(),
            key: "eyes".into(),
            value: "green".into(),
            scope_path: None,
            status: upstream::ClaimStatus::Accepted,
            provenance: upstream::Provenance::Witnessed,
            supersedes_id: None,
            entity_id: None,
            evidence: None,
            confidence: None,
            shape_ref: None,
            origin: None,
        });
        snapshot.anchors.insert(
            "hero.eyes".into(),
            upstream::Anchor {
                id: "anchor".into(),
                version_id: "v1".into(),
                detail_key: "hero.eyes".into(),
                locked_value: "green".into(),
                locked_at_scope: None,
                status: upstream::AnchorStatus::Locked,
                seq: 1,
                evidence: None,
            },
        );
        let text = "The chapter opens with rain on the harbor and the bell ringing twice.";
        let candidate = upstream::Candidate {
            text: format!("As an AI. {text}"),
            previous_texts: vec![format!("As an AI. {text}")],
            claims: vec![upstream::ProposedClaim {
                claim_type: upstream::ClaimType::Fact,
                subject: "hero".into(),
                key: "eyes".into(),
                value: "blue".into(),
                supersedes_id: None,
            }],
            corrections: vec![upstream::ProposedClaim {
                claim_type: upstream::ClaimType::Correction,
                subject: "villain".into(),
                key: "name".into(),
                value: "Mora".into(),
                supersedes_id: None,
            }],
            ..Default::default()
        };
        let findings = run_gates(&snapshot, &candidate);
        assert_eq!(
            findings
                .iter()
                .map(|f| f.rule_id.as_str())
                .collect::<Vec<_>>(),
            vec![RULE_ANCHOR, RULE_ORPHAN, RULE_META, RULE_SIMILARITY]
        );
        assert_eq!(findings[0].severity, upstream::Severity::Block);
        assert!(findings[1..]
            .iter()
            .all(|f| f.severity == upstream::Severity::Warn));
        let mut equivalent = candidate;
        equivalent.text.clear();
        equivalent.previous_texts.clear();
        equivalent.corrections.clear();
        equivalent.claims[0].value = "  Green  ".into();
        assert!(run_gates(&snapshot, &equivalent).is_empty());
    }
}
