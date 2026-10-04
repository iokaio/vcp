// SPDX-License-Identifier: Apache-2.0
//! Durable failed-request identities and late charge observations. No transport
//! runs under the canonical worker; trusted host metadata GETs feed raw receipts.
use super::*;
use crate::foundation::reconciliation::{PendingCharge, ReconciliationStatus};
use serde::{Deserialize, Serialize};
use vcp_models::reconciliation::{self, Expected};

const FAILED_SCHEMA: &str = "failed-provider-request/1";
const RECEIPT_SCHEMA: &str = "provider-generation-charge/1";
/// Three uncertain availability submissions already equal the normal model-step
/// attempt ceiling. Keep this root-wide bound across successful intervening steps.
const MAX_UNRESOLVED_AVAILABILITY: usize = 3;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailedRequest {
    attempt: AttemptId,
    scope: Scope,
    request_id: Option<String>,
    raw_response: ArtifactId,
    availability: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(
        temp: &tempfile::TempDir,
        backend: vcp_store::BackendKind,
    ) -> (Context, ThreadBinding) {
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let currency: Currency = "USD".to_owned().try_into().unwrap();
        let config = Config {
            canonical_root: temp.path().join("canonical"),
            backend,
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            binding: Binding {
                host: HostId::new(),
                root: workspace.to_string_lossy().into_owned(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
            actor: ActorId::new(),
            root_task: TaskId::new(),
            cap: Money {
                currency: currency.clone(),
                micros: Micros::new(2000),
            }.into(),
            protected: Micros::new(200),
            price: PriceSnapshot {
                id: "a".repeat(64),
                provider: "fixture/region".into(),
                model: "fixture/model".into(),
                currency,
                capability: "b".repeat(64),
                valid_until: Timestamp::new(u64::MAX),
                rates: [
                    ChargeCategory::Input,
                    ChargeCategory::Output,
                    ChargeCategory::CacheRead,
                    ChargeCategory::CacheWrite,
                    ChargeCategory::Request,
                    ChargeCategory::ProviderTool,
                ]
                .into_iter()
                .map(|category| {
                    (
                        category,
                        Rate {
                            micros: Micros::new(if category == ChargeCategory::Request {
                                100
                            } else {
                                0
                            }),
                            per_units: Units::new(1),
                        },
                    )
                })
                .collect(),
            },
            input_ceiling: Units::new(1000),
            output_ceiling: Units::new(100),
            artifact_limit: ByteCount::new(16 * 1024 * 1024),
            max_transport_retries: 2,
            host_tool_denials: vec![],
        };
        let mut context = Context::open(config).unwrap();
        let binding = ThreadBinding {
            scope: Scope {
                workspace: context.config.workspace.clone(),
                session: context.config.session.clone(),
                task: context.config.root_task.clone(),
            },
            agent: AgentId::new(),
            role: RequestRole::Main,
        };
        context
            .command(
                Command::CreateTask {
                    root: binding.scope.task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "contain unresolved availability across model steps".into(),
                        constraints: vec![],
                        acceptance: vec!["preserve protected verification".into()],
                        source: EventId::new(),
                        steering: SteeringRevision::ZERO,
                    },
                    fingerprint: vcp_domain::verification::Fingerprint {
                        repository: "a".repeat(64),
                        buffers: "b".repeat(64),
                        environment: "c".repeat(64),
                    },
                    editing: false,
                    required_checks: vec![],
                },
                Some(binding.scope.task.clone()),
                Revision::ZERO,
            )
            .unwrap();
        context
            .command(
                Command::Transition {
                    next: TaskState::Running,
                    reason: "fixture".into(),
                    verification: None,
                },
                Some(binding.scope.task.clone()),
                Revision::ZERO,
            )
            .unwrap();
        (context, binding)
    }
    fn failed(context: &mut Context, binding: &ThreadBinding, index: usize) -> AttemptId {
        let (attempt, _, _, _) = context
            .admit(
                binding,
                serde_json::json!({"model":"fixture/model","input":[],"tools":[]}),
            )
            .unwrap();
        context
            .response_error_chunk(
                &attempt,
                format!(r#"{{"id":"gen-failed-{index}"}}"#).as_bytes(),
            )
            .unwrap();
        let mut writer = context.streams.remove(&attempt).unwrap();
        let descriptor = writer.abort().unwrap();
        drop(writer);
        context
            .command(
                Command::AttachArtifact {
                    descriptor: descriptor.clone(),
                },
                Some(binding.scope.task.clone()),
                Revision::ZERO,
            )
            .unwrap();
        context
            .record_failed_charge(
                &attempt,
                &descriptor,
                Some(&vcp_models::retry::ProviderFailure {
                    failure: vcp_models::retry::Failure::RateLimit,
                    http_status: Some(429),
                    limit_source: Some(vcp_models::retry::LimitSource::UpstreamProviderSharedPool),
                    retry_after_ms: None,
                }),
                None,
            )
            .unwrap();
        let actor = context.actor();
        context
            .runtime
            .block_on(vcp_budget::hold_uncertain(
                context.engine.store_mut(),
                &attempt,
                &binding.scope,
                &actor,
                "availability charge unknown",
            ))
            .unwrap();
        attempt
    }

    #[test]
    fn interrupted_sse_identity_reopens_and_settles_without_new_inference() {
        for backend in [
            vcp_store::BackendKind::Files,
            vcp_store::BackendKind::Sqlite,
        ] {
            for legacy in [false, true] {
                for (cost, micros) in [("0", 0), ("0.00005", 50)] {
                    let temp = tempfile::tempdir().unwrap();
                    let (mut context, binding) = setup(&temp, backend);
                    let (attempt, _, _, _) = context
                        .admit(
                            &binding,
                            serde_json::json!({"model":"fixture/model","input":[],"tools":[]}),
                        )
                        .unwrap();
                    let frame = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"gen-interrupted\"}}\n\n";
                    let mut bytes = frame.to_vec();
                    bytes.extend(format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"{}\"}}\n\n", "x".repeat(90_000)).as_bytes());
                    bytes.extend_from_slice(b"data: {\"type\":\"response.in_progress\"");
                    for chunk in bytes.chunks(65_536) {
                        context.response_error_chunk(&attempt, chunk).unwrap();
                    }
                    let mut writer = context.streams.remove(&attempt).unwrap();
                    let raw = writer.abort().unwrap();
                    drop(writer);
                    context
                        .command(
                            Command::AttachArtifact {
                                descriptor: raw.clone(),
                            },
                            Some(binding.scope.task.clone()),
                            Revision::ZERO,
                        )
                        .unwrap();
                    let generation = vcp_models::stream::retained_generation(&bytes)
                        .unwrap()
                        .unwrap();
                    if legacy {
                        // Exact old schema with its missing generation identity.
                        context
                            .capture(
                                &binding.scope,
                                Channel::Evidence,
                                &canonical_bytes(
                                    &serde_json::json!({"attempt":attempt,"scope":binding.scope,
                                "request_id":null,"raw_response":raw.spec.id,"availability":false}),
                                )
                                .unwrap(),
                                FAILED_SCHEMA,
                            )
                            .unwrap();
                    } else {
                        context
                            .record_failed_charge(&attempt, &raw, None, Some(&generation))
                            .unwrap();
                    }
                    let actor = context.actor();
                    context
                        .runtime
                        .block_on(vcp_budget::hold_uncertain(
                            context.engine.store_mut(),
                            &attempt,
                            &binding.scope,
                            &actor,
                            "interrupted SSE",
                        ))
                        .unwrap();
                    let settings = context.config.clone();
                    context.close().unwrap();
                    let mut reopened = Context::open(settings).unwrap();
                    let pending = reopened.pending_provider_charges().unwrap().pop().unwrap();
                    assert_eq!(pending.expected.request_id, "gen-interrupted");
                    assert_eq!(
                        reopened
                            .reconciliation_artifact(&raw.spec.id, 16 * 1024 * 1024)
                            .unwrap(),
                        bytes
                    );
                    let receipt = format!(r#"{{"data":{{"id":"gen-interrupted","total_cost":{cost},"finish_reason":"stop"}}}}"#).into_bytes();
                    for _ in 0..2 {
                        reopened
                            .reconcile_provider_receipt(pending.clone(), receipt.clone())
                            .unwrap();
                    }
                    let state = reopened.engine.store().state();
                    let ledger = vcp_budget::ledger(state, &binding.scope).unwrap();
                    assert_eq!(
                        (
                            ledger.settled.get(),
                            ledger.active.get(),
                            ledger.unresolved.get()
                        ),
                        (micros, 0, 0)
                    );
                    assert_eq!(
                        state
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Attempt)
                            .count(),
                        1
                    );
                    assert_eq!(
                        state
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Settlement)
                            .count(),
                        1
                    );
                    reopened.close().unwrap();
                }
            }
        }
    }
    #[test]
    fn markerless_terminal_without_cost_reopens_with_exact_receipt_provenance() {
        for backend in [
            vcp_store::BackendKind::Files,
            vcp_store::BackendKind::Sqlite,
        ] {
            for mode in [
                "valid",
                "conflicting_id",
                "wrong_source",
                "wrong_channel",
                "duplicate",
                "header_conflict",
            ] {
                let temp = tempfile::tempdir().unwrap();
                let (mut context, binding) = setup(&temp, backend);
                let (attempt, _, _, _) = context
                    .admit(
                        &binding,
                        serde_json::json!({"model":"fixture/model","input":[],"tools":[]}),
                    )
                    .unwrap();
                // Match the real provider's created/in_progress/failed framing:
                // a complete failed terminal contains an ID but no observed cost.
                let terminal_id = if mode == "conflicting_id" {
                    "gen-other"
                } else {
                    "gen-failed-without-cost"
                };
                let bytes = format!(concat!(
                    "data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"gen-failed-without-cost\",\"status\":\"in_progress\"}}}}\n\n",
                    "data: {{\"type\":\"response.in_progress\",\"response\":{{\"id\":\"gen-failed-without-cost\",\"status\":\"in_progress\"}}}}\n\n",
                    "data: {{\"type\":\"response.failed\",\"response\":{{\"id\":\"{}\",\"status\":\"failed\",\"output\":[],\"usage\":null,\"error\":{{\"code\":\"server_error\"}}}}}}\n\ndata: [DONE]\n\n"
                ), terminal_id).into_bytes();
                if mode == "header_conflict" {
                    context
                        .record_failed_generation_header(&attempt, "gen-other".into())
                        .unwrap();
                }
                let mut writer = context.streams.remove(&attempt).unwrap();
                writer.write_chunk(&bytes).unwrap();
                let raw = writer.finalize().unwrap();
                drop(writer);
                // Old complete_provider attached this exact capture and held
                // uncertainty, but never created failed-provider-request/1.
                if mode != "wrong_source" && mode != "wrong_channel" {
                    context
                        .command(
                            Command::AttachArtifact {
                                descriptor: raw.clone(),
                            },
                            Some(binding.scope.task.clone()),
                            Revision::ZERO,
                        )
                        .unwrap();
                }
                if matches!(mode, "wrong_source" | "wrong_channel" | "duplicate") {
                    let mut spec = raw.spec.clone();
                    spec.id = ArtifactId::new();
                    if mode == "wrong_source" {
                        spec.source = "unrelated-response".into();
                    }
                    if mode == "wrong_channel" {
                        spec.channel = Channel::Evidence;
                    }
                    let mut other = context.engine.store().spool().create(spec).unwrap();
                    other.write_chunk(&bytes).unwrap();
                    let descriptor = other.finalize().unwrap();
                    drop(other);
                    context
                        .command(
                            Command::AttachArtifact { descriptor },
                            Some(binding.scope.task.clone()),
                            Revision::ZERO,
                        )
                        .unwrap();
                }
                let actor = context.actor();
                context
                    .runtime
                    .block_on(vcp_budget::hold_uncertain(
                        context.engine.store_mut(),
                        &attempt,
                        &binding.scope,
                        &actor,
                        "failed terminal omitted cost",
                    ))
                    .unwrap();
                let settings = context.config.clone();
                context.close().unwrap();
                let mut reopened = Context::open(settings).unwrap();
                let pending = reopened.pending_provider_charges();
                if matches!(mode, "duplicate" | "wrong_channel") {
                    assert!(pending.is_err(), "{mode}");
                } else if mode != "valid" {
                    assert!(pending.unwrap().is_empty(), "{mode}");
                } else {
                    let pending = pending.unwrap();
                    assert_eq!(pending.len(), 1);
                    assert_eq!(pending[0].expected.request_id, "gen-failed-without-cost");
                    let mut wrong_scope = binding.scope.clone();
                    wrong_scope.session = SessionId::new();
                    assert!(
                        reopened
                            .recover_failed_identity(&FailedRequest {
                                attempt: attempt.clone(),
                                scope: wrong_scope,
                                request_id: None,
                                raw_response: raw.spec.id.clone(),
                                availability: false,
                            })
                            .is_err()
                    );
                    // The lookup must read and hash-check the retained bytes,
                    // rather than trusting their descriptor or a previous read.
                    let chunk = std::fs::read_dir(
                        reopened
                            .config
                            .canonical_root
                            .join("spool")
                            .join(raw.spec.id.as_str()),
                    )
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.extension()
                            .is_some_and(|extension| extension == "chunk")
                    })
                    .unwrap();
                    let original = std::fs::read(&chunk).unwrap();
                    let mut altered = original.clone();
                    altered[0] ^= 1;
                    std::fs::write(&chunk, &altered).unwrap();
                    assert!(reopened.pending_provider_charges().unwrap().is_empty());
                    std::fs::write(&chunk, &original).unwrap();
                    let ledger =
                        vcp_budget::ledger(reopened.engine.store().state(), &binding.scope)
                            .unwrap();
                    assert_eq!(
                        (
                            ledger.settled.get(),
                            ledger.active.get(),
                            ledger.unresolved.get()
                        ),
                        (0, 0, 100)
                    );
                    assert_eq!(
                        reopened
                            .reconciliation_artifact(&raw.spec.id, 65536)
                            .unwrap(),
                        bytes
                    );
                    reopened.reconcile_provider_receipt(pending[0].clone(),
                        br#"{"data":{"id":"gen-failed-without-cost","total_cost":0.00005,"finish_reason":"error"}}"#.to_vec()).unwrap();
                    let ledger =
                        vcp_budget::ledger(reopened.engine.store().state(), &binding.scope)
                            .unwrap();
                    assert_eq!(
                        (
                            ledger.settled.get(),
                            ledger.active.get(),
                            ledger.unresolved.get()
                        ),
                        (50, 0, 0)
                    );
                    assert_eq!(
                        reopened
                            .engine
                            .store()
                            .state()
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Attempt)
                            .count(),
                        1
                    );
                }
                reopened.close().unwrap();
            }
        }
    }

    #[test]
    fn provider_liability_guard_survives_successful_steps_and_reopen_preserving_protected_money() {
        for backend in [
            vcp_store::BackendKind::Files,
            vcp_store::BackendKind::Sqlite,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let (mut context, binding) = setup(&temp, backend);
            for index in 0..3 {
                failed(&mut context, &binding, index);
                if index < 2 {
                    context.guard_unresolved_availability(&binding).unwrap();
                    // A successful unrelated inference step cannot reset the
                    // preceding failures or recover their uncertain balance.
                    let (successful, _, _, _) = context
                        .admit(
                            &binding,
                            serde_json::json!({"model":"fixture/model","input":[],"tools":[]}),
                        )
                        .unwrap();
                    let raw = context
                        .capture(
                            &binding.scope,
                            Channel::Evidence,
                            b"actual final usage",
                            "fixture-usage/1",
                        )
                        .unwrap();
                    context
                        .observe_usage(UsageObservation {
                            id: ObservationId::new(),
                            scope: binding.scope.clone(),
                            attempt: successful.clone(),
                            provider_request: format!("gen-success-{index}"),
                            mode: UsageMode::Cumulative {
                                version: Units::new(1),
                            },
                            amount: Money {
                                currency: context.config.cap.currency.clone(),
                                micros: Micros::new(10),
                            },
                            final_usage: true,
                            raw: raw.spec.id,
                            correction: None,
                        })
                        .unwrap();
                    if let Some(mut writer) = context.streams.remove(&successful) {
                        writer.abort().unwrap();
                    }
                }
            }
            let before = context
                .engine
                .store()
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .count();
            assert!(
                context
                    .admit(
                        &binding,
                        serde_json::json!({"model":"fixture/model","input":[],"tools":[]})
                    )
                    .unwrap_err()
                    .to_string()
                    .contains("liability threshold")
            );
            assert_eq!(
                before,
                context
                    .engine
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Attempt)
                    .count()
            );
            let ledger =
                vcp_budget::ledger(context.engine.store().state(), &binding.scope).unwrap();
            assert_eq!(
                (
                    ledger.settled.get(),
                    ledger.unresolved.get(),
                    ledger.protected.get()
                ),
                (20, 300, 200)
            );
            let settings = context.config.clone();
            context.close().unwrap();
            let mut reopened = Context::open(settings).unwrap();
            assert!(reopened.guard_unresolved_availability(&binding).is_err());
            let mut verification = binding.clone();
            verification.role = RequestRole::Verification;
            reopened
                .guard_unresolved_availability(&verification)
                .unwrap();
            let pending = reopened.pending_provider_charges().unwrap().pop().unwrap();
            let raw = format!(
                r#"{{"data":{{"id":"{}","total_cost":0,"finish_reason":"error"}}}}"#,
                pending.expected.request_id
            )
            .into_bytes();
            reopened.reconcile_provider_receipt(pending, raw).unwrap();
            reopened.guard_unresolved_availability(&binding).unwrap();
            assert_eq!(
                vcp_budget::ledger(reopened.engine.store().state(), &binding.scope)
                    .unwrap()
                    .protected
                    .get(),
                200
            );
            reopened.close().unwrap();
        }
    }
}

impl Context {
    pub fn record_failed_generation_header(
        &mut self,
        attempt: &AttemptId,
        id: String,
    ) -> Result<()> {
        if !reconciliation::valid_request_id(&id) {
            return Err("invalid failed generation header".into());
        }
        let admitted =
            vcp_budget::attempt(self.engine.store().current(), attempt, &self.config.workspace)?;
        if admitted.root != self.config.root_task || admitted.phase != ReservationState::Submitted {
            return Err("generation header lacks current submitted attempt".into());
        }
        self.capture(
            &admitted.scope,
            Channel::Evidence,
            &canonical_bytes(&serde_json::json!({"attempt":attempt,"request_id":id}))?,
            "failed-generation-header/1",
        )?;
        Ok(())
    }

    fn reconciliation_artifact(&self, id: &ArtifactId, limit: u64) -> Result<Vec<u8>> {
        let descriptor: ArtifactDescriptor = self
            .engine
            .store()
            .current()
            .record(Collection::Artifact, id.as_str(), &self.config.workspace)?
            .decode()?;
        if descriptor.length.get() > limit {
            return Err("charge evidence byte limit".into());
        }
        let mut bytes = Vec::new();
        self.runtime
            .block_on(vcp_audit::history::History::read_artifact(
                self.engine.store(),
                &self.history_access(),
                id,
                &mut bytes,
            ))?;
        Ok(bytes)
    }

    pub(super) fn record_failed_charge(
        &mut self,
        attempt: &AttemptId,
        raw: &ArtifactDescriptor,
        failure: Option<&vcp_models::retry::ProviderFailure>,
        generation: Option<&vcp_models::stream::ObservedGeneration>,
    ) -> Result<()> {
        let admitted =
            vcp_budget::attempt(self.engine.store().current(), attempt, &self.config.workspace)?;
        if admitted.scope != raw.spec.scope {
            return Err("failed response scope differs".into());
        }
        // A missing/malformed identity remains an uncertain charge. Record the
        // failure even when no metadata lookup can be made.
        let headers = self.failed_generation_headers(attempt, &admitted.scope)?;
        let request_id = if headers.len() > 1 {
            None
        } else if let Some(generation) = generation {
            if headers
                .first()
                .is_some_and(|id| id != &generation.request_id)
            {
                None
            } else {
                Some(generation.request_id.clone())
            }
        } else {
            self.reconciliation_artifact(&raw.spec.id, 64 * 1024)
                .ok()
                .and_then(|bytes| {
                    reconciliation::failed_request_id(&bytes, headers.first().map(String::as_str))
                        .ok()
                        .flatten()
                })
        };
        let availability = failure.is_some_and(|f| {
            matches!(
                f.failure,
                vcp_models::retry::Failure::RateLimit
                    | vcp_models::retry::Failure::Transient
                    | vcp_models::retry::Failure::Timeout
            )
        });
        self.capture(
            &admitted.scope,
            Channel::Evidence,
            &canonical_bytes(&FailedRequest {
                attempt: attempt.clone(),
                scope: admitted.scope.clone(),
                request_id,
                raw_response: raw.spec.id.clone(),
                availability,
            })?,
            FAILED_SCHEMA,
        )?;
        Ok(())
    }

    fn failed_generation_headers(
        &self,
        attempt: &AttemptId,
        scope: &Scope,
    ) -> Result<std::collections::BTreeSet<String>> {
        let mut headers = std::collections::BTreeSet::new();
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = row.decode()?;
            if &descriptor.spec.scope != scope
                || descriptor.spec.schema != "failed-generation-header/1"
            {
                continue;
            }
            let header: serde_json::Value =
                serde_json::from_slice(&self.reconciliation_artifact(&descriptor.spec.id, 4096)?)?;
            if header["attempt"] == attempt.as_str() {
                if let Some(id) = header["request_id"]
                    .as_str()
                    .filter(|id| reconciliation::valid_request_id(id))
                {
                    headers.insert(id.to_owned());
                }
            }
        }
        Ok(headers)
    }

    fn failed_charges(&self) -> Result<Vec<FailedRequest>> {
        let mut failed = std::collections::BTreeMap::new();
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.spec.scope.workspace != self.config.workspace
                || descriptor.spec.schema != FAILED_SCHEMA
            {
                continue;
            }
            let marker: FailedRequest =
                serde_json::from_slice(&self.reconciliation_artifact(&descriptor.spec.id, 4096)?)?;
            let attempt = vcp_budget::attempt(
                self.engine.store().current(),
                &marker.attempt,
                &self.config.workspace,
            )?;
            if attempt.scope != marker.scope || descriptor.spec.scope != marker.scope {
                return Err("failed request accounting scope conflict".into());
            }
            if attempt.root != self.config.root_task {
                continue;
            }
            if attempt.phase == ReservationState::ReconciliationPending {
                failed.insert(marker.attempt.clone(), marker);
            }
        }
        // Older owners finalized a terminal response before discovering its
        // missing charge, leaving a pending attempt without a FailedRequest.
        // Recover only an exact, uniquely linked complete response descriptor;
        // recover_failed_identity still verifies its bytes, framing and headers.
        // This supplies lookup provenance only, never a charge or new marker.
        let state = self.engine.store().current();
        for row in state.records.values().filter(|row| {
            row.collection == Collection::Attempt && row.workspace == self.config.workspace
        }) {
            let attempt: Attempt = row.decode()?;
            if attempt.root != self.config.root_task
                || attempt.phase != ReservationState::ReconciliationPending
                || failed.contains_key(&attempt.id)
            {
                continue;
            }
            let source = super::capture_recovery::source(&attempt.id);
            let mut response = None;
            for row in state.records.values().filter(|row| {
                row.collection == Collection::Artifact && row.workspace == self.config.workspace
            }) {
                let descriptor: ArtifactDescriptor = row.decode()?;
                if descriptor.spec.source != source {
                    continue;
                }
                if descriptor.spec.scope != attempt.scope
                    || descriptor.spec.channel != Channel::Response
                    || descriptor.spec.schema != "responses-sse-observed-through-terminal/1"
                    || descriptor.state != CaptureState::Complete
                {
                    return Err("markerless response accounting scope or capture conflict".into());
                }
                if response.replace(descriptor.spec.id).is_some() {
                    return Err("ambiguous markerless response accounting capture".into());
                }
            }
            if let Some(raw_response) = response {
                failed.insert(
                    attempt.id.clone(),
                    FailedRequest {
                        attempt: attempt.id,
                        scope: attempt.scope,
                        request_id: None,
                        raw_response,
                        availability: false,
                    },
                );
            }
        }
        Ok(failed.into_values().collect())
    }

    pub(super) fn guard_unresolved_availability(&mut self, binding: &ThreadBinding) -> Result<()> {
        // Verification retains its existing protected allowance. Reconciliation
        // itself creates no inference reservation and works while paused.
        if binding.role == RequestRole::Verification {
            return Ok(());
        }
        if self
            .failed_charges()?
            .iter()
            .filter(|failed| failed.availability)
            .count()
            >= MAX_UNRESOLVED_AVAILABILITY
        {
            self.pause_root("reconciliation required: three unresolved provider availability failures; protected verification balance retained")?;
            return Err("provider availability liability threshold reached; reconcile original requests before further inference".into());
        }
        Ok(())
    }

    pub fn pending_provider_charges(&self) -> Result<Vec<PendingCharge>> {
        let mut pending = Vec::new();
        for failed in self.failed_charges()? {
            let Some(request_id) = self.recover_failed_identity(&failed)? else {
                continue;
            };
            let admitted = vcp_budget::attempt(
                self.engine.store().current(),
                &failed.attempt,
                &self.config.workspace,
            )?;
            let mut expected = Expected {
                request_id,
                model: admitted.quote.price.model.clone(),
                endpoint: admitted.quote.price.provider.clone(),
                provider_name: None,
                model_revision: None,
            };
            if let Some(bytes) = self.original_charge_catalog(&admitted.quote.price)? {
                let catalog: serde_json::Value = serde_json::from_slice(&bytes)?;
                if catalog["data"]["id"] != expected.model {
                    return Err("original charge catalog model mismatch".into());
                }
                if let Some(endpoint) = catalog["data"]["endpoints"]
                    .as_array()
                    .and_then(|endpoints| endpoints.iter().find(|e| e["tag"] == expected.endpoint))
                {
                    let provider = endpoint["provider_name"]
                        .as_str()
                        .filter(|p| p.len() <= 256);
                    let revision = endpoint["name"]
                        .as_str()
                        .and_then(|n| n.split_once(" | "))
                        .filter(|(p, _)| Some(*p) == provider)
                        .map(|(_, revision)| revision);
                    if let Some(provider) = provider {
                        expected.provider_name = Some(provider.into());
                    }
                    if let Some(revision) = revision.filter(|r| r.len() <= 256) {
                        expected.model_revision = Some(revision.into());
                    }
                }
            }
            pending.push(PendingCharge {
                attempt: failed.attempt,
                scope: failed.scope,
                expected,
            });
        }
        Ok(pending)
    }

    fn recover_failed_identity(&self, failed: &FailedRequest) -> Result<Option<String>> {
        if failed.request_id.is_some() {
            return Ok(failed.request_id.clone());
        }
        // Legacy owners captured the exact partial response but did not retain
        // successful SSE generation identities. Read the original hash-checked
        // artifact under current history access; never rewrite that evidence.
        let raw: ArtifactDescriptor = self
            .engine
            .store()
            .current()
            .record(
                Collection::Artifact,
                failed.raw_response.as_str(),
                &self.config.workspace,
            )?
            .decode()?;
        if raw.spec.scope != failed.scope
            || raw.spec.channel != Channel::Response
            || raw.spec.source != super::capture_recovery::source(&failed.attempt)
        {
            return Err("failed response artifact scope, channel or attempt conflict".into());
        }
        let headers = self.failed_generation_headers(&failed.attempt, &failed.scope)?;
        if headers.len() > 1 {
            return Ok(None);
        }
        let bytes = match self.reconciliation_artifact(&failed.raw_response, 16 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        let generation = vcp_models::stream::retained_generation(&bytes)
            .ok()
            .flatten();
        Ok(generation
            .filter(|value| {
                headers
                    .first()
                    .is_none_or(|header| header == &value.request_id)
            })
            .map(|value| value.request_id))
    }

    fn original_charge_catalog(&self, price: &PriceSnapshot) -> Result<Option<Vec<u8>>> {
        // Match the exact admission price snapshot ID, never a fresh catalog for
        // the same model or a provider display name with ambiguous regions.
        let mut found: Option<Vec<u8>> = None;
        for row in self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.spec.scope.workspace != self.config.workspace {
                continue;
            }
            let bytes = match descriptor.spec.schema.as_str() {
                "routing-catalog-sources/1" => {
                    let sources: std::collections::BTreeMap<String, String> =
                        serde_json::from_slice(
                            &self.reconciliation_artifact(&descriptor.spec.id, 16 * 1024 * 1024)?,
                        )?;
                    sources.get(&price.id).map(|raw| raw.as_bytes().to_vec())
                }
                "openrouter-provider-configuration/1" => {
                    let configuration: serde_json::Value = serde_json::from_slice(
                        &self.reconciliation_artifact(&descriptor.spec.id, 64 * 1024)?,
                    )?;
                    let snapshot: vcp_models::catalog::Snapshot =
                        serde_json::from_value(configuration["snapshot"].clone())?;
                    if snapshot.price == *price {
                        let id: ArtifactId =
                            serde_json::from_value(configuration["catalog_artifact"].clone())?;
                        let raw = self.reconciliation_artifact(&id, 4 * 1024 * 1024)?;
                        if vcp_protocol::digest_bytes(&raw) != snapshot.raw_sha256 {
                            return Err("original charge catalog digest differs".into());
                        }
                        Some(raw)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(bytes) = bytes {
                if bytes.len() > 4 * 1024 * 1024
                    || found.as_ref().is_some_and(|prior| prior != &bytes)
                {
                    return Err("conflicting or oversized original charge catalogs".into());
                }
                found = Some(bytes);
            }
        }
        Ok(found)
    }

    pub fn reconcile_provider_receipt(
        &mut self,
        pending: PendingCharge,
        raw: Vec<u8>,
    ) -> Result<ReconciliationStatus> {
        let admitted = vcp_budget::attempt(
            self.engine.store().current(),
            &pending.attempt,
            &self.config.workspace,
        )?;
        if admitted.scope != pending.scope
            || admitted.quote.price.model != pending.expected.model
            || admitted.quote.price.provider != pending.expected.endpoint
        {
            return Err("charge receipt admission differs".into());
        }
        let receipt = reconciliation::charge(&raw, &pending.expected)?;
        if receipt.amount.currency != admitted.quote.amount.currency {
            return Err("charge receipt currency differs from ledger".into());
        }
        let observation_id = ObservationId::parse(vcp_protocol::digest_bytes(&canonical_bytes(
            &(&pending.attempt, &receipt.request_id, &receipt.raw_sha256),
        )?))?;
        if let Some(row) = self.engine.store().current().records.values().find(|row| {
            row.collection == Collection::Settlement && row.id == observation_id.as_str()
        }) {
            let settled: Settlement = row.decode()?;
            if settled.attempt != pending.attempt || settled.total != receipt.amount.micros {
                return Err("charge receipt settlement conflict".into());
            }
            return Ok(ReconciliationStatus::Settled {
                attempt: pending.attempt,
                amount: receipt.amount.micros,
            });
        }
        // Recheck durable captured identity, including after another task changed
        // catalog/current sources while this metadata GET was in flight.
        let current = self
            .pending_provider_charges()?
            .into_iter()
            .find(|p| p.attempt == pending.attempt)
            .ok_or("charge attempt no longer pending")?;
        if current.expected != pending.expected {
            return Err("charge receipt identity changed".into());
        }
        let existing = self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .map(|row| row.decode::<ArtifactDescriptor>())
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .find(|d| {
                d.spec.schema == RECEIPT_SCHEMA
                    && d.spec.scope == pending.scope
                    && d.sha256 == receipt.raw_sha256
            });
        let descriptor = match existing {
            Some(descriptor) => descriptor,
            None => self.capture(&pending.scope, Channel::Evidence, &raw, RECEIPT_SCHEMA)?,
        };
        #[cfg(feature = "qualification")]
        self.qualification_model_dispatch_point(
            crate::foundation::model_dispatch_qualification::Point::BeforeSettlement,
            &pending.attempt,
        )?;
        let actor = self.actor();
        let ledger = vcp_budget::ledger(self.engine.store().current(), &pending.scope)?;
        self.observe_usage(UsageObservation {
            id: observation_id,
            scope: pending.scope,
            attempt: pending.attempt.clone(),
            provider_request: receipt.request_id,
            mode: UsageMode::Cumulative {
                version: Units::new(
                    admitted
                        .usage_watermark
                        .get()
                        .checked_add(1)
                        .ok_or("charge observation version overflow")?,
                ),
            },
            amount: receipt.amount.clone(),
            final_usage: true,
            raw: descriptor.spec.id,
            correction: Some(Resolution {
                actor: actor.id,
                policy: ledger.policy,
                reason: format!(
                    "authoritative generation total charge; normalization {}; receipt {}",
                    receipt.normalization, receipt.raw_sha256
                ),
                remaining_uncertainty: String::new(),
            }),
        })?;
        Ok(ReconciliationStatus::Settled {
            attempt: pending.attempt,
            amount: receipt.amount.micros,
        })
    }

    pub fn replay_provider_receipts(&mut self) -> Result<Vec<ReconciliationStatus>> {
        let pending = self.pending_provider_charges()?;
        let receipts = self
            .engine
            .store()
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
            .map(|row| row.decode::<ArtifactDescriptor>())
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|d| d.spec.schema == RECEIPT_SCHEMA)
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for pending in pending {
            for descriptor in receipts.iter().filter(|d| d.spec.scope == pending.scope) {
                let raw = self.reconciliation_artifact(&descriptor.spec.id, 64 * 1024)?;
                if reconciliation::charge(&raw, &pending.expected).is_ok() {
                    result.push(self.reconcile_provider_receipt(pending.clone(), raw)?);
                    break;
                }
            }
        }
        Ok(result)
    }
}
