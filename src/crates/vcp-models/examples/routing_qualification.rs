// SPDX-License-Identifier: Apache-2.0
//! Offline policy assertions. Hypothetical fixture qualification is never saved
//! as a live catalog and never dispatches an actual model task.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    time::Instant,
};
use vcp_domain::{
    accounting::{Money, RequestRole, Usage},
    *,
};
use vcp_models::{
    catalog::{Compatibility, Snapshot},
    routing::*,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MANIFEST: &[u8] = include_bytes!("../../../evals/routing/manifest.json");
const STRATEGIES: [&str; 3] = ["fixed_economical", "fixed_stronger", "routed"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    revision: String,
    declared_before_execution: bool,
    purpose: String,
    criteria: Criteria,
    models: Vec<Model>,
    tasks: Vec<Case>,
    remote_comparisons: Vec<Value>,
    release_enablement: String,
    rollback_triggers: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Criteria {
    quality_floor_bps: u16,
    minimum_samples: u32,
    maximum_evidence_age_ms: u64,
    required_decision_assertion_rate_bps: u16,
    permitted_human_interventions: u32,
    maximum_estimated_retry_requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    estimated_helper_micros: u64,
    estimated_compaction_micros: u64,
    estimated_optimizer_micros: u64,
    estimated_verification_micros: u64,
    estimated_children_micros: u64,
    estimated_handoff_requests: u64,
    profile: Profile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    id: String,
    endpoint: String,
    group: Group,
    input_usd_per_token: String,
    output_usd_per_token: String,
    request_usd: String,
    synthetic_p95_ms: u64,
    synthetic_samples: u32,
    synthetic_quality_bps: BTreeMap<String, u16>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    partition: String,
    task_class: String,
    input: String,
    now: u64,
    budget_micros: u64,
    protected_micros: u64,
    denied_models: Vec<String>,
    economical_retry_requests: u64,
    economical_support_known: bool,
    economical_tools: State,
    expected: BTreeMap<String, Expected>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    eligible: Vec<String>,
    selected: Option<String>,
}
fn money(micros: u64) -> Result<Money> {
    Ok(Money {
        currency: "USD".to_string().try_into()?,
        micros: Micros::new(micros),
    })
}
fn usage(criteria: &Criteria, requests: u64) -> Result<Usage> {
    Ok(Usage {
        input: Units::new(
            criteria
                .input_tokens
                .checked_mul(requests)
                .ok_or("input bound overflow")?,
        ),
        output: Units::new(
            criteria
                .output_tokens
                .checked_mul(requests)
                .ok_or("output bound overflow")?,
        ),
        requests: Units::new(requests),
        ..Usage::default()
    })
}
fn provenance() -> Vec<Provenance> {
    vec![Provenance {
    source:"fixture://p6-04-offline-routing-v1".into(),sha256:vcp_protocol::digest_bytes(MANIFEST),
    observed_at:Timestamp::new(10),effective_at:None,
    limitations:vec!["Scripted hypothetical input only: no actual model, measured task quality or production qualification".into()],
}]
}
fn candidate(model: &Model, case: &Case, hypothetical: bool) -> Result<Candidate> {
    let compatibility = Compatibility {
        id: format!("synthetic-compatibility-{}", model.id),
        model: model.id.clone(),
        endpoint: model.endpoint.clone(),
        qualified_at: Timestamp::new(1),
        valid_until: Timestamp::new(1000),
        responses_text_tools: true,
        byte_ceiling_qualified: true,
        provider_preferences_qualified: true,
        deny_data_collection: true,
        require_zdr: true,
        request_price_limit: model.request_usd.clone(),
        required_parameters: BTreeSet::from(["tools".into()]),
        qualified_reasoning_efforts: BTreeSet::new(),
    };
    let raw = vcp_protocol::canonical_bytes(
        &json!({"data":{"id":model.id,"endpoints":[{"tag":model.endpoint,"status":0,"context_length":4000,"max_prompt_tokens":3000,"max_completion_tokens":1000,"supported_parameters":["tools"],"pricing":{"prompt":model.input_usd_per_token,"completion":model.output_usd_per_token,"request":model.request_usd}}]}}),
    )?;
    let snapshot = Snapshot::from_endpoints(
        &raw,
        Timestamp::new(10),
        Timestamp::new(1000),
        compatibility,
    )?;
    // The second phase is a counterfactual over production's closed evidence
    // discriminant, not a qualification operation. Neither catalog is persisted.
    let kind = if hypothetical {
        EvidenceKind::Live
    } else {
        EvidenceKind::Scripted
    };
    let observation = CompatibilityObservation {
        id: format!("synthetic-probe-{}", model.id),
        compatibility: snapshot.compatibility.id.clone(),
        kind,
        state: State::Supported,
        observed_at: Timestamp::new(10),
        valid_until: Timestamp::new(1000),
        provenance: provenance(),
    };
    let roles = model
        .synthetic_quality_bps
        .iter()
        .map(|(class, quality)| RoleEvidence {
            id: format!("synthetic-role-{}-{class}", model.id),
            role: RequestRole::Main,
            task_class: class.clone(),
            kind,
            observed_at: Timestamp::new(10),
            valid_until: Timestamp::new(1000),
            samples: model.synthetic_samples,
            quality_bps: *quality,
            latency_p50_ms: model.synthetic_p95_ms,
            latency_p95_ms: model.synthetic_p95_ms,
            usage_p50: None,
            usage_p95: None,
            provenance: provenance(),
        })
        .collect();
    Ok(Candidate {
        identity: ModelEndpoint {
            model: model.id.clone(),
            endpoint: model.endpoint.clone(),
        },
        availability: State::Supported,
        reasons: vec![],
        provenance: provenance(),
        capabilities: BTreeMap::from([(
            "tools".into(),
            if model.id == "fixture/economical" {
                case.economical_tools
            } else {
                State::Supported
            },
        )]),
        snapshot: Some(snapshot),
        compatibility: vec![observation],
        memberships: vec![GroupMembership {
            version: format!("synthetic-group-{}", model.id),
            group: model.group,
            roles,
        }],
    })
}
fn policy(
    catalog: &CatalogRevision,
    manifest: &Manifest,
    case: &Case,
    strategy: &str,
) -> Result<Policy> {
    let pin = match strategy {
        "fixed_economical" => Some("fixture/economical"),
        "fixed_stronger" => Some("fixture/stronger"),
        "routed" => None,
        _ => return Err("undeclared strategy".into()),
    };
    Ok(Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: manifest.criteria.profile,
        allowed_models: catalog
            .entries
            .iter()
            .filter(|entry| !case.denied_models.contains(&entry.identity.model))
            .map(|entry| entry.identity.model.clone())
            .collect(),
        allowed_endpoints: catalog
            .entries
            .iter()
            .map(|entry| entry.identity.endpoint.clone())
            .collect(),
        allowed_groups: BTreeSet::from([Group::Low, Group::High]),
        quality_floor_bps: manifest.criteria.quality_floor_bps,
        minimum_samples: manifest.criteria.minimum_samples,
        maximum_evidence_age_ms: manifest.criteria.maximum_evidence_age_ms,
        deny_data_collection: true,
        require_zdr: true,
        ordering: vec![
            Preference::TotalCost,
            Preference::Latency,
            Preference::Quality,
            Preference::Capability,
        ],
        pin: pin.map(|model| Pin {
            candidate: catalog
                .entries
                .iter()
                .find(|entry| entry.identity.model == model)
                .expect("validated fixed fixture model")
                .identity
                .clone(),
            fallback_candidates: BTreeSet::new(),
        }),
        broader_task_class: None,
        output_tokens: None,
        input_tokens: None,
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()?)
}
fn routing_input(
    catalog: &CatalogRevision,
    policy: &Policy,
    manifest: &Manifest,
    case: &Case,
) -> Result<RoutingInput> {
    let criteria = &manifest.criteria;
    let support = criteria
        .estimated_helper_micros
        .checked_add(criteria.estimated_compaction_micros)
        .and_then(|value| value.checked_add(criteria.estimated_optimizer_micros))
        .ok_or("support estimate overflow")?;
    let estimates = catalog
        .entries
        .iter()
        .map(|entry| {
            Ok(CostEstimate {
                candidate: entry.identity.clone(),
                first_attempt: usage(criteria, 1)?,
                retries: usage(
                    criteria,
                    if entry.identity.model == "fixture/economical" {
                        case.economical_retry_requests
                    } else {
                        0
                    },
                )?,
                handoff: usage(criteria, criteria.estimated_handoff_requests)?,
                support: if entry.identity.model == "fixture/economical"
                    && !case.economical_support_known
                {
                    None
                } else {
                    Some(money(support)?)
                },
                children: Some(money(criteria.estimated_children_micros)?),
                verification: Some(money(criteria.estimated_verification_micros)?),
                assumptions: vec![
                    "Synthetic declared upper bounds; no cost is claimed to have been incurred"
                        .into(),
                    format!(
                        "helper={} compaction={} optimizer={} micros; all declared in manifest",
                        criteria.estimated_helper_micros,
                        criteria.estimated_compaction_micros,
                        criteria.estimated_optimizer_micros
                    ),
                ],
                evidence_refs: vec![manifest.revision.clone()],
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(RoutingInput {
        retry_pin: None,
        workspace: WorkspaceId::parse("offline-routing-fixture")?,
        root: TaskId::parse(&case.id)?,
        task: TaskId::parse(&case.id)?,
        input_revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        input_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&(
            &case.id,
            &case.task_class,
            &case.input,
        ))?),
        catalog: catalog.id.clone(),
        policy: policy.id.clone(),
        role: RequestRole::Main,
        task_class: case.task_class.clone(),
        now: Timestamp::new(case.now),
        required_capabilities: BTreeSet::from(["tools".into()]),
        excluded: BTreeSet::new(),
        input_tokens: Units::new(criteria.input_tokens),
        output_tokens: Units::new(criteria.output_tokens),
        available: money(case.budget_micros)?,
        protected_verification: Micros::new(case.protected_micros),
        estimates,
    })
}
fn validate(manifest: &Manifest) -> Result<()> {
    if !manifest.declared_before_execution
        || manifest.criteria.required_decision_assertion_rate_bps != 10_000
        || manifest.criteria.permitted_human_interventions != 0
        || manifest.criteria.profile != Profile::Low
    {
        return Err(
            "fixture requires frozen all-pass deterministic criteria and no interventions".into(),
        );
    }
    let models: BTreeSet<_> = manifest
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect();
    if models != BTreeSet::from(["fixture/economical", "fixture/stronger"])
        || manifest.models.len() != 2
    {
        return Err("fixture has exactly two synthetic fixed identities".into());
    }
    let mut ids = BTreeSet::new();
    let mut partitions = BTreeSet::new();
    for case in &manifest.tasks {
        if !ids.insert(&case.id)
            || !matches!(case.partition.as_str(), "tuning" | "held_out")
            || !matches!(
                case.task_class.as_str(),
                "analysis" | "review" | "generation"
            )
            || case.economical_retry_requests > manifest.criteria.maximum_estimated_retry_requests
            || case.expected.len() != STRATEGIES.len()
        {
            return Err("invalid task partition, retry cap or expectations".into());
        }
        partitions.insert((case.partition.as_str(), case.task_class.as_str()));
        for strategy in STRATEGIES {
            let expected = case
                .expected
                .get(strategy)
                .ok_or("missing independent strategy assertion")?;
            if expected
                .eligible
                .iter()
                .any(|id| !models.contains(id.as_str()))
                || expected
                    .selected
                    .as_ref()
                    .is_some_and(|id| !expected.eligible.contains(id))
                || expected.eligible.iter().collect::<BTreeSet<_>>().len()
                    != expected.eligible.len()
            {
                return Err("invalid expected candidate identities".into());
            }
        }
    }
    if partitions.len() != 6 {
        return Err("each partition must contain analysis/review/generation cases".into());
    }
    Ok(())
}
fn percentile(values: &mut [u64], percent: usize) -> Option<u64> {
    values.sort_unstable();
    if values.is_empty() {
        None
    } else {
        Some(values[(values.len() * percent).div_ceil(100).saturating_sub(1)])
    }
}
fn setup_failure(
    case: &Case,
    strategy: &str,
    phase: &str,
    order: usize,
    error: impl std::fmt::Display,
) -> Value {
    json!({"execution_order":order,"phase":phase,"partition":case.partition,"case":case.id,
        "task_class":case.task_class,"task_input":case.input,"strategy":strategy,
        "decision_assertion_pass":false,"selector_wall_ns":null,"decision":null,
        "error":format!("setup: {error}"),"abandoned":false,"human_interventions":0,
        "actual_model_attempts":[],"actual_task_outcome":"not_run","actual_model_cost":null,"actual_model_latency_ms":null})
}
fn run() -> Result<Value> {
    let manifest: Manifest = serde_json::from_slice(MANIFEST)?;
    validate(&manifest)?;
    let mut attempts = Vec::new();
    let mut artifacts = BTreeMap::new();
    let mut order = 0usize;
    for hypothetical in [false, true] {
        let phase = if hypothetical {
            "hypothetical_qualified_fixture"
        } else {
            "scripted_evidence_rejection"
        };
        for case in &manifest.tasks {
            let catalog = (|| -> Result<CatalogRevision> {
                Ok(CatalogRevision::create(
                    None,
                    Timestamp::new(20),
                    None,
                    manifest
                        .models
                        .iter()
                        .map(|model| candidate(model, case, hypothetical))
                        .collect::<Result<Vec<_>>>()?,
                )?)
            })();
            if let Ok(catalog) = &catalog {
                artifacts.insert(
                    catalog.id.clone(),
                    json!({"kind":"catalog","value":catalog}),
                );
            }
            for strategy in STRATEGIES {
                order += 1;
                let catalog = match &catalog {
                    Ok(catalog) => catalog,
                    Err(error) => {
                        attempts.push(setup_failure(case, strategy, phase, order, error));
                        continue;
                    }
                };
                let prepared = (|| -> Result<_> {
                    let policy = policy(catalog, &manifest, case, strategy)?;
                    let input = routing_input(catalog, &policy, &manifest, case)?;
                    Ok((policy, input))
                })();
                let (policy, input) = match prepared {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        attempts.push(setup_failure(case, strategy, phase, order, error));
                        continue;
                    }
                };
                artifacts.insert(policy.id.clone(), json!({"kind":"policy","value":policy}));
                let start = Instant::now();
                let result = select(catalog, &policy, &input);
                let elapsed = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
                let blocked = Expected {
                    eligible: vec![],
                    selected: None,
                };
                let expected = if hypothetical {
                    case.expected
                        .get(strategy)
                        .ok_or("missing frozen assertion")?
                } else {
                    &blocked
                };
                let (decision, error, pass) = match result {
                    Err(error) => (Value::Null, Some(error.to_string()), false),
                    Ok(decision) => {
                        let actual: BTreeSet<_> = decision
                            .candidates
                            .iter()
                            .filter(|row| row.exclusions.is_empty())
                            .map(|row| row.identity.model.clone())
                            .collect();
                        let expected_set: BTreeSet<_> = expected.eligible.iter().cloned().collect();
                        let selected = decision.selected.as_ref().map(|identity| &identity.model);
                        let gate = actual == expected_set
                            && selected == expected.selected.as_ref()
                            && decision.immediate_reservation.is_none()
                            && (hypothetical
                                || decision.candidates.iter().all(|row| {
                                    row.exclusions
                                        .contains(&Exclusion::MissingLiveQualification)
                                }));
                        (serde_json::to_value(decision)?, None, gate)
                    }
                };
                attempts.push(json!({"execution_order":order,"phase":phase,"partition":case.partition,"case":case.id,"task_class":case.task_class,"task_input":case.input,"task_input_sha256":input.input_digest,"strategy":strategy,"expected":expected,"decision_assertion_pass":pass,"selector_wall_ns":elapsed,"decision":decision,"error":error,"abandoned":false,"human_interventions":0,"actual_model_attempts":[],"actual_task_outcome":"not_run","actual_model_cost":null,"actual_model_latency_ms":null}));
            }
        }
    }
    let mut summaries = Vec::new();
    for phase in [
        "scripted_evidence_rejection",
        "hypothetical_qualified_fixture",
    ] {
        for partition in ["tuning", "held_out"] {
            for strategy in STRATEGIES {
                let rows: Vec<_> = attempts
                    .iter()
                    .filter(|row| {
                        row["phase"] == phase
                            && row["partition"] == partition
                            && row["strategy"] == strategy
                    })
                    .collect();
                let mut timings: Vec<_> = rows
                    .iter()
                    .filter_map(|row| row["selector_wall_ns"].as_u64())
                    .collect();
                let p50 = percentile(&mut timings, 50);
                let p95 = percentile(&mut timings, 95);
                summaries.push(json!({"phase":phase,"partition":partition,"strategy":strategy,"attempted_decisions":rows.len(),"expected_selection_or_stop_passes":rows.iter().filter(|row|row["decision_assertion_pass"]==true).count(),"errors":rows.iter().filter(|row|!row["error"].is_null()).count(),"selector_wall_ns_p50":p50,"selector_wall_ns_p95":p95,"actual_task_outcomes":"not_run","actual_model_cost":null}));
            }
        }
    }
    let pass = attempts
        .iter()
        .all(|row| row["decision_assertion_pass"] == true);
    Ok(
        json!({"schema":"p6-04-offline-routing-report/1","revision":manifest.revision,"purpose":manifest.purpose,"pass":pass,
        "manifest_sha256":vcp_protocol::digest_bytes(MANIFEST),"harness_sha256":vcp_protocol::digest_bytes(include_bytes!("routing_qualification.rs")),
        "production_source_sha256":{"routing_types":vcp_protocol::digest_bytes(include_bytes!("../src/routing.rs")),"selector":vcp_protocol::digest_bytes(include_bytes!("../src/routing/selection.rs")),"validation":vcp_protocol::digest_bytes(include_bytes!("../src/routing/validation.rs")),"endpoint_catalog":vcp_protocol::digest_bytes(include_bytes!("../src/catalog.rs"))},
        "environment":{"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"logical_parallelism":std::thread::available_parallelism().ok().map(|value|value.get())},
        "harness_model_calls":0,"harness_model_spend_usd":0,"actual_task_comparison_model_cost":null,"tuning_performed":false,
        "proposed_production_defaults":null,"production_enablement":manifest.release_enablement,"remote_comparisons":manifest.remote_comparisons,
        "rollback_triggers":manifest.rollback_triggers,"limitations":["Offline deterministic selector assertions only; task prompts are not submitted or solved","Both catalogs and all quality/price/latency inputs are synthetic; no actual model is qualified","Hypothetical phase uses in-memory Live discriminants solely to test conditional production selector behavior; scripted phase proves actual Scripted evidence cannot select","Selector wall time is measured; synthetic evidence latency is not model latency","No fees, outputs, retries, child events or human interventions are fabricated as actual attempts","No tuning/calibration fitting, live profile thresholds, remote evaluator qualification or P8 delegation/package evidence provided"],
        "artifacts":artifacts,"summary":summaries,"attempts":attempts}),
    )
}
fn main() -> Result<()> {
    let report = match run() {
        Ok(report) => report,
        Err(error) => {
            json!({"schema":"p6-04-offline-routing-report/1","pass":false,"manifest_sha256":vcp_protocol::digest_bytes(MANIFEST),"setup_error":error.to_string(),"attempts":[]})
        }
    };
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(path) = std::env::args_os().nth(1) {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    } else {
        println!("{}", String::from_utf8(bytes)?);
    }
    if report["pass"] != true {
        return Err(
            "offline selector assertions failed; inspect complete attempted-decision report".into(),
        );
    }
    Ok(())
}
