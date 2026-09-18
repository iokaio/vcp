// SPDX-License-Identifier: Apache-2.0
//! Volatile qualification adapter, not the durable VCP canonical store.
use super::{digest, Corpus, Result};
use munarium_core::{
    gates::run_gates,
    ledger::FactQuery,
    storage::{load_snapshot, FindingsQuery, NewClaim, StorageBackend},
    types::{Candidate, Claim, ClaimStatus, ClaimType, ProposedClaim, Severity},
};
use munarium_store_mem::MemStore;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

struct ScopedLedger {
    workspace: String,
    store: MemStore,
    version: String,
    records: BTreeMap<String, Claim>,
}
impl ScopedLedger {
    async fn new(workspace: &str) -> Result<Self> {
        let store = MemStore::new();
        let version = store
            .create_version(None, Some(json!({"workspace":workspace})))
            .await?;
        Ok(Self {
            workspace: workspace.into(),
            store,
            version,
            records: BTreeMap::new(),
        })
    }
    async fn head(&self) -> Result<u64> {
        Ok(self.store.head(&self.version).await?)
    }
    // Exclusive mutable access serializes the snapshot/gate/write sequence. These
    // separate volatile writes do NOT qualify durable proposal/finding atomicity.
    async fn submit(
        &mut self,
        workspace: &str,
        id: &str,
        subject: &str,
        text: &str,
        supersedes: Option<&str>,
        expected_head: u64,
    ) -> Result<Claim> {
        if workspace != self.workspace {
            return Err("workspace authority mismatch".into());
        }
        if self.records.contains_key(id) {
            return Err("duplicate document identity".into());
        }
        if self.head().await? != expected_head {
            return Err("stale canonical head".into());
        }
        let snapshot =
            load_snapshot(&self.store, &self.version, None, None, Some(expected_head)).await?;
        let target = if let Some(target_id) = supersedes {
            let target = self
                .records
                .get(target_id)
                .ok_or("unknown scoped supersession target")?;
            if target.subject != subject || !snapshot.facts.iter().any(|c| c.id == target.id) {
                return Err("supersession target is not current for this subject".into());
            }
            Some(target.id.clone())
        } else {
            None
        };
        let proposed = ProposedClaim {
            claim_type: if target.is_some() {
                ClaimType::Correction
            } else {
                ClaimType::Fact
            },
            subject: subject.into(),
            key: "description".into(),
            value: text.into(),
            supersedes_id: target.clone(),
        };
        let mut candidate = Candidate {
            scope_path: Some(workspace.into()),
            text: text.into(),
            ..Default::default()
        };
        if target.is_some() {
            candidate.corrections.push(proposed);
        } else {
            candidate.claims.push(proposed);
        }
        let findings = run_gates(&snapshot, &candidate);
        let blocked = findings.iter().any(|f| f.severity == Severity::Block);
        let mut claim = NewClaim::fact(subject, "description", text);
        claim.claim_type = if target.is_some() {
            ClaimType::Correction
        } else {
            ClaimType::Fact
        };
        claim.scope_path = Some(workspace.into());
        claim.status = if blocked {
            ClaimStatus::Disputed
        } else {
            ClaimStatus::Accepted
        };
        // Upstream resolve_slice includes disputed edges in its superseded set.
        // A rejected proposal is evidence, never an effective replacement edge.
        claim.supersedes_id = if blocked { None } else { target };
        claim.evidence = Some(json!({"kind":"synthetic_fixture", "workspace":workspace,
            "document_id":id, "text_sha256":digest(text.as_bytes()),
            "proposed_supersedes_document_id":supersedes, "findings":findings}));
        let stored = self
            .store
            .append_claim(&self.version, claim, Some(expected_head))
            .await?;
        self.store
            .record_findings(&self.version, stored.seq, &findings)
            .await?;
        self.records.insert(id.into(), stored.clone());
        Ok(stored)
    }
    async fn visible(&self, workspace: &str, pin: Option<u64>) -> Result<BTreeSet<String>> {
        if workspace != self.workspace {
            return Err("workspace authority mismatch".into());
        }
        let facts = self
            .store
            .slice_facts(
                &self.version,
                &FactQuery {
                    as_of_seq: pin,
                    ..Default::default()
                },
            )
            .await?;
        let mut visible = BTreeSet::new();
        for fact in facts {
            let (id, original) = self
                .records
                .iter()
                .find(|(_, c)| c.id == fact.id)
                .ok_or("unknown canonical claim")?;
            if original != &fact || fact.scope_path.as_deref() != Some(workspace) {
                return Err("canonical provenance mismatch".into());
            }
            visible.insert(id.clone());
        }
        Ok(visible)
    }
}

pub(super) struct View {
    current: BTreeMap<String, BTreeSet<String>>,
    pub report: Value,
}
impl View {
    pub fn contains(&self, workspace: &str, id: &str) -> bool {
        self.current
            .get(workspace)
            .is_some_and(|ids| ids.contains(id))
    }
}
pub(super) fn from_corpus(corpus: &Corpus) -> Result<View> {
    tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
        let mut current = BTreeMap::new();
        let mut reports = Vec::new();
        for workspace in corpus.documents.iter().map(|d| d.workspace.as_str()).collect::<BTreeSet<_>>() {
            let mut ledger = ScopedLedger::new(workspace).await?;
            let mut subjects = BTreeMap::<String, String>::new();
            let mut historical = 0;
            for document in corpus.documents.iter().filter(|d| d.workspace == workspace) {
                let subject = match &document.supersedes {
                    Some(id) => subjects.get(id).ok_or("supersession must name a previous scoped document")?.clone(),
                    None => document.id.clone(),
                };
                let before = ledger.head().await?;
                let claim = ledger.submit(workspace, &document.id, &subject, &document.text, document.supersedes.as_deref(), before).await?;
                if claim.status != ClaimStatus::Accepted { return Err("corpus proposal unexpectedly disputed".into()); }
                subjects.insert(document.id.clone(), subject);
                if let Some(old) = &document.supersedes {
                    let pinned = ledger.visible(workspace, Some(before)).await?;
                    let latest = ledger.visible(workspace, None).await?;
                    if !pinned.contains(old) || pinned.contains(&document.id) || latest.contains(old) || !latest.contains(&document.id) {
                        return Err("historical supersession resolution failed".into());
                    }
                    let original = ledger.records.get(old).ok_or("missing historical record")?;
                    if ledger.store.get_claim(&original.id).await?.as_ref() != Some(original) { return Err("superseded evidence was lost".into()); }
                    historical += 1;
                }
            }
            let ids = ledger.visible(workspace, None).await?;
            // `current` remains an independent fixture oracle; it does not select
            // production results or feed the retained resolver.
            let expected: BTreeSet<String> = corpus.documents.iter().filter(|d| d.workspace == workspace && d.current).map(|d| d.id.clone()).collect();
            if ids != expected { return Err("governed visibility differs from independent fixture truth".into()); }
            reports.push(json!({"workspace":workspace,"recorded":ledger.records.len(),"current_ids":ids,
                "historical_checks":historical,"findings":ledger.store.findings(&ledger.version, &FindingsQuery::default()).await?.len()}));
            current.insert(workspace.into(), ids);
        }
        Ok(View { current, report: json!({"backend":"munarium-store-mem", "durable":false, "workspaces":reports}) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(future: impl std::future::Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future);
    }
    #[test]
    fn rejected_correction_retains_accepted_value_proposal_and_findings() {
        run(async {
            let mut ledger = ScopedLedger::new("atlas").await.unwrap();
            let original = ledger
                .submit("atlas", "old", "pause", "keep terminal open", None, 0)
                .await
                .unwrap();
            ledger
                .store
                .lock_anchor(
                    &ledger.version,
                    "pause",
                    "description",
                    "keep terminal open",
                    Some("atlas"),
                    None,
                )
                .await
                .unwrap();
            let head = ledger.head().await.unwrap();
            let rejected = ledger
                .submit(
                    "atlas",
                    "rejected",
                    "pause",
                    "close terminal",
                    Some("old"),
                    head,
                )
                .await
                .unwrap();
            assert_eq!(rejected.status, ClaimStatus::Disputed);
            assert!(rejected.supersedes_id.is_none());
            assert_eq!(
                rejected.evidence.as_ref().unwrap()["proposed_supersedes_document_id"],
                "old"
            );
            assert_eq!(
                ledger.visible("atlas", None).await.unwrap(),
                BTreeSet::from(["old".into()])
            );
            assert_eq!(
                ledger.store.get_claim(&original.id).await.unwrap(),
                Some(original)
            );
            let findings = ledger
                .store
                .findings(&ledger.version, &FindingsQuery::default())
                .await
                .unwrap();
            assert!(findings
                .iter()
                .any(|f| f.finding.rule_id == "gate.anchor-consistency" && f.seq == rejected.seq));
        });
    }
    #[test]
    fn conflicting_observation_is_recorded_without_changing_current_truth() {
        run(async {
            let mut ledger = ScopedLedger::new("atlas").await.unwrap();
            ledger
                .submit("atlas", "first", "pause", "keep terminal open", None, 0)
                .await
                .unwrap();
            let rejected = ledger
                .submit("atlas", "conflict", "pause", "close terminal", None, 1)
                .await
                .unwrap();
            assert_eq!(rejected.status, ClaimStatus::Disputed);
            assert_eq!(
                ledger.visible("atlas", None).await.unwrap(),
                BTreeSet::from(["first".into()])
            );
            assert_eq!(
                ledger.store.get_claim(&rejected.id).await.unwrap(),
                Some(rejected)
            );
        });
    }
    #[test]
    fn foreign_authority_and_stale_or_unknown_supersession_have_no_effect() {
        run(async {
            let mut ledger = ScopedLedger::new("atlas").await.unwrap();
            ledger
                .submit("atlas", "first", "pause", "open", None, 0)
                .await
                .unwrap();
            for (workspace, target, head) in [
                ("boreal", None, 1),
                ("atlas", None, 0),
                ("atlas", Some("foreign"), 1),
            ] {
                assert!(ledger
                    .submit(workspace, "bad", "pause", "closed", target, head)
                    .await
                    .is_err());
                assert_eq!(ledger.head().await.unwrap(), 1);
                assert_eq!(ledger.records.len(), 1);
            }
            assert!(ledger.visible("boreal", None).await.is_err());
        });
    }
    #[test]
    fn replacement_requires_current_matching_subject_and_unique_identity() {
        run(async {
            let mut ledger = ScopedLedger::new("atlas").await.unwrap();
            ledger
                .submit("atlas", "first", "pause", "open", None, 0)
                .await
                .unwrap();
            assert!(ledger
                .submit("atlas", "different", "budget", "closed", Some("first"), 1)
                .await
                .is_err());
            assert_eq!(ledger.head().await.unwrap(), 1);
            ledger
                .submit("atlas", "second", "pause", "paused", Some("first"), 1)
                .await
                .unwrap();
            assert!(ledger
                .submit("atlas", "third", "pause", "closed", Some("first"), 2)
                .await
                .is_err());
            assert!(ledger
                .submit("atlas", "second", "pause", "closed", Some("second"), 2)
                .await
                .is_err());
            assert_eq!(ledger.head().await.unwrap(), 2);
            assert_eq!(ledger.records.len(), 2);
            assert_eq!(
                ledger.visible("atlas", None).await.unwrap(),
                BTreeSet::from(["second".into()])
            );
            assert_eq!(
                ledger.visible("atlas", Some(1)).await.unwrap(),
                BTreeSet::from(["first".into()])
            );
        });
    }
    #[test]
    fn corpus_replay_preserves_history_and_uses_governed_visibility() {
        let corpus = super::super::corpus().unwrap();
        let view = from_corpus(&corpus).unwrap();
        assert!(view.contains("atlas", "atlas-pause-v2"));
        assert!(!view.contains("atlas", "atlas-pause-v1"));
        assert!(!view.contains("boreal", "atlas-pause-v2"));
        assert_eq!(view.report["workspaces"][0]["historical_checks"], 1);
    }
}
