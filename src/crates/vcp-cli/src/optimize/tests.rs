// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{ActorId, AuthorityRevision, DeletionEpoch, Watermark, WorkspaceId};
use vcp_lifecycle::foundation::routing_state::{Counts, HistoryWindow};

#[test]
fn transitions_uses_only_the_read_only_evidence_request() {
    assert_eq!(parse(&["transitions"]), Ok(Command::Transitions));
    let mut calls = 0;
    let text = Session::default().execute(Command::Transitions, Timestamp::new(20), |request| {
        calls += 1;
        assert!(matches!(request, Request::Transitions { from: None, until } if until == Timestamp::new(20)));
        Ok(serde_json::json!({"alphabet":"canonical-task-state/1","transitions":[]}))
    }).unwrap();
    assert_eq!(calls, 1);
    assert!(text.contains("canonical-task-state/1"));
}

#[test]
fn cycles_uses_only_the_read_only_evidence_request() {
    assert_eq!(parse(&["cycles"]), Ok(Command::Cycles));
    assert!(parse(&["cycles", "apply"]).is_err());
    let mut calls = 0;
    let text = Session::default()
        .execute(Command::Cycles, Timestamp::new(20), |request| {
            calls += 1;
            assert!(matches!(request, Request::Cycles { from: None, until } if until == Timestamp::new(20)));
            Ok(serde_json::json!({"cycles":[],"remote_requests":0}))
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        serde_json::json!({"cycles":[],"remote_requests":0})
    );
    assert!(HELP.contains("/optimize cycles"));
}

#[test]
fn forecasts_preserves_source_and_abstention_without_mutation_requests() {
    assert_eq!(parse(&["forecasts"]), Ok(Command::Forecasts));
    for argument in ["apply", "--provider", "--quality-floor"] {
        assert!(parse(&["forecasts", argument]).is_err());
    }
    let expected = serde_json::json!({
        "source_evidence":"retained-source", "source_cutoff":"15",
        "forecast":null, "abstention":"sparse", "unknown_liabilities":true,
        "serving_qualified":false, "limitations":["observed costs are not forecasts"]
    });
    let mut calls = 0;
    let text = Session::default().execute(Command::Forecasts, Timestamp::new(20), |request| {
        calls += 1;
        assert!(matches!(request, Request::Forecasts { from: None, until } if until == Timestamp::new(20)));
        Ok(expected.clone())
    }).unwrap();
    assert_eq!(calls, 1);
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), expected);
    assert!(HELP.contains("/optimize forecasts"));
    assert!(Session::default()
        .execute(Command::Forecasts, Timestamp::new(20), |_| {
            Err("retained source unavailable".into())
        })
        .is_err());
}

#[test]
fn observations_uses_only_the_read_only_evidence_request() {
    assert_eq!(parse(&["observations"]), Ok(Command::Observations));
    let mut calls = 0;
    let text = Session::default()
        .execute(Command::Observations, Timestamp::new(20), |request| {
            calls += 1;
            assert!(matches!(request, Request::Observations { from: None, until } if until == Timestamp::new(20)));
            Ok(serde_json::json!({"alphabet":"canonical-action-observation/2","attempts":[]}))
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert!(text.contains("canonical-action-observation/2"));
}

fn policy(profile: Profile, quality: u16) -> Policy {
    Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile,
        allowed_models: BTreeSet::new(),
        allowed_endpoints: BTreeSet::new(),
        allowed_groups: BTreeSet::new(),
        quality_floor_bps: quality,
        minimum_samples: 10,
        maximum_evidence_age_ms: 1000,
        deny_data_collection: true,
        require_zdr: true,
        ordering: order(profile),
        pin: None,
        broader_task_class: None,
        output_tokens: None,
        input_tokens: None,
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()
    .unwrap()
}
fn interview() -> Interview {
    Interview {
        version: 1,
        revision: Revision::ZERO,
        answers: BTreeMap::new(),
    }
}
fn report() -> OptimizationReport {
    OptimizationReport {
        forecast: None,
        source_tasks: None,
        observed: Default::default(),
        id: "report-fixture".into(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        cutoff: Watermark::new(5),
        window: HistoryWindow {
            from: None,
            until: Timestamp::new(10),
        },
        counts: Counts {
            tasks: 3,
            failed: 1,
            attempts: 4,
            uncertain_attempts: 1,
            ..Counts::default()
        },
        cohorts: BTreeMap::from([(r#"{"size":"unknown"}"#.into(), 3)]),
        cohort_denominator: "all retained tasks".into(),
        tasks: BTreeSet::new(),
        evidence: vec![],
        uncertainty: vec!["Synthetic incomplete observation".into()],
    }
}

#[test]
fn saved_forecast_sidecars_and_drift_keep_observed_totals_and_no_action_requests() {
    let mut calls = Vec::new();
    let text = Session::default().execute(Command::Report, Timestamp::new(10), |request| {
        let value = match request {
            Request::Status => { calls.push("status"); serde_json::json!({"policy":null}) },
            Request::Report { .. } => {
                calls.push("report");
                serde_json::json!({"report":report(),"interview":interview(),"next_question":"priority",
                    "forecast":{"source":"saved-forecast","cohorts":[],"serving_qualified":false},
                    "compaction":{"source":"saved-compaction","entries":[],"serving_qualified":false}})
            },
            _ => panic!("report must not request a policy action or provider"),
        };
        Ok(value)
    }).unwrap();
    assert_eq!(calls, vec!["status", "report"]);
    assert!(text.contains("3 tasks"));
    assert!(text.contains("saved-forecast"));
    assert!(text.contains("saved-compaction"));
    assert!(text.contains("unqualified"));
    let value = serde_json::json!({"automatic_action":false,"forecast_drift":null,"forecast_drift_unavailable":"legacy snapshot"});
    let mut count = 0;
    let rendered = Session::default()
        .execute(
            Command::Compare {
                baseline: "a".into(),
                current: "b".into(),
            },
            Timestamp::new(10),
            |request| {
                assert!(matches!(request, Request::Compare { .. }));
                count += 1;
                Ok(value.clone())
            },
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(serde_json::from_str::<Value>(&rendered).unwrap(), value);
}
fn preview() -> Preview {
    Preview {
        base: Revision::ZERO,
        report: "report-fixture".into(),
        selected: vec![
            Edit::Profile(Profile::High),
            Edit::Ordering(order(Profile::High)),
            Edit::QualityFloorBps(9000),
        ],
        prior: policy(Profile::Low, 8000),
        persisted: policy(Profile::High, 9000),
        effective: policy(Profile::High, 9500),
        ceilings_digest: "c".repeat(64),
        reason: "Explicit requested changes".into(),
        uncertainty: vec!["Coverage incomplete".into()],
        interview: interview(),
    }
}
fn receipt(command: CommandId) -> ApplyReceipt {
    ApplyReceipt {
        command,
        digest: "d".repeat(64),
        published: Published {
            revision: Revision::new(1),
            parent: Some(Revision::ZERO),
            actor: ActorId::parse("owner").unwrap(),
            authority: AuthorityRevision::ZERO,
            timestamp: Timestamp::new(10),
            value: policy(Profile::High, 9000),
        },
        effective: policy(Profile::High, 9500),
    }
}

#[test]
fn parser_requires_explicit_floor_and_rollback_revision_without_json() {
    assert_eq!(
        parse(&["compare", "baseline", "current"]),
        Ok(Command::Compare {
            baseline: "baseline".into(),
            current: "current".into()
        })
    );
    assert_eq!(parse(&[]), Ok(Command::Report));
    assert_eq!(parse(&["status"]), Ok(Command::Status));
    assert_eq!(
        parse(&["answer", "priority", "Prefer", "lower", "total", "cost"]),
        Ok(Command::Answer {
            question: Question::Priority,
            value: "Prefer lower total cost".into()
        })
    );
    assert_eq!(
        parse(&["preview", "med", "--quality-floor", "9000"]),
        Ok(Command::Preview {
            profile: Profile::Med,
            quality_floor_bps: 9000
        })
    );
    assert_eq!(
        parse(&["rollback", "2", "--expected", "7"]),
        Ok(Command::Rollback {
            target: Revision::new(2),
            expected: Revision::new(7)
        })
    );
    for input in [
        vec!["preview", "low"],
        vec!["preview", "low", "--quality-floor", "10001"],
        vec!["preview", "high", "--quality-floor", "-1"],
        vec!["preview", "high", "--quality-floor", "95.5"],
        vec!["rollback", "2"],
        vec!["rollback", "2", "--expected", "-1"],
        vec!["answer", "priority"],
        vec!["answer", "unknown", "x"],
        vec!["apply", "yes"],
        vec!["apply", "{}"],
    ] {
        assert!(parse(&input).is_err(), "{input:?}");
    }
}

#[test]
fn selected_fields_are_explicit_bounded_and_do_not_imply_other_changes() {
    use vcp_models::routing::{Group, ModelEndpoint, Pin};
    assert_eq!(
        parse(&["preview", "--output-tokens", "128"]),
        Ok(Command::PreviewSelected {
            selected: vec![Edit::OutputTokens(Some(vcp_domain::Units::new(128)))]
        })
    );
    assert_eq!(
        parse(&["preview", "--output-tokens", "inherit"]),
        Ok(Command::PreviewSelected {
            selected: vec![Edit::OutputTokens(None)]
        })
    );
    assert!(parse(&["preview", "--output-tokens", "0"]).is_err());
    assert_eq!(
        parse(&["preview", "--unpin", "--quality-floor", "8000"]),
        Ok(Command::PreviewSelected {
            selected: vec![Edit::Pin(None), Edit::QualityFloorBps(8000)]
        })
    );
    assert_eq!(
        parse(&[
            "preview",
            "--minimum-samples",
            "30",
            "--models",
            "b,a",
            "--endpoints",
            "none",
            "--groups",
            "high,low",
            "--pin",
            "a",
            "provider"
        ]),
        Ok(Command::PreviewSelected {
            selected: vec![
                Edit::MinimumSamples(30),
                Edit::AllowedModels(BTreeSet::from(["a".into(), "b".into()])),
                Edit::AllowedEndpoints(BTreeSet::new()),
                Edit::AllowedGroups(BTreeSet::from([Group::High, Group::Low])),
                Edit::Pin(Some(Pin {
                    candidate: ModelEndpoint {
                        model: "a".into(),
                        endpoint: "provider".into()
                    },
                    fallback_candidates: BTreeSet::new()
                })),
            ]
        })
    );
    assert_eq!(
        parse(&["preview", "--unpin"]),
        Ok(Command::PreviewSelected {
            selected: vec![Edit::Pin(None)]
        })
    );
    for words in [
        vec!["preview"],
        vec!["preview", "--models", "a,a"],
        vec!["preview", "--models", "a,"],
        vec!["preview", "--models", "a b"],
        vec!["preview", "--groups", "med"],
        vec!["preview", "--pin", "a"],
        vec!["preview", "--unpin", "--pin", "a", "b"],
        vec!["preview", "--quality-floor", "1", "--quality-floor", "2"],
        vec!["preview", "--minimum-samples", "0"],
        vec!["preview", "--maximum-evidence-age-ms", "0"],
        vec!["preview", "--budget", "100"],
        vec!["preview", "--remote-advice", "true"],
    ] {
        assert!(parse(&words).is_err(), "{words:?}");
    }
    let large = (0..33)
        .map(|i| format!("model{i}"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(parse(&["preview", "--models", &large]).is_err());
}

#[test]
fn resource_controls_parse_explicit_values_inheritance_and_reject_invalid_bounds() {
    use vcp_domain::{ByteCount, Units};
    use vcp_models::{reasoning::Effort, routing::RetrievalLimits};
    assert_eq!(
        parse(&[
            "preview",
            "--input-tokens",
            "1024",
            "--max-quality-switches",
            "0",
            "--minimum-repeated-failures",
            "3",
            "--reasoning-effort",
            "low",
            "--retrieval-limits",
            "2",
            "500",
            "1000"
        ]),
        Ok(Command::PreviewSelected {
            selected: vec![
                Edit::InputTokens(Some(Units::new(1024))),
                Edit::EscalationMaxQualitySwitches(Some(0)),
                Edit::EscalationMinimumRepeatedFailures(Some(3)),
                Edit::ReasoningEffort(Some(Effort::Low)),
                Edit::RetrievalLimits(Some(RetrievalLimits {
                    results: 2,
                    tokens: Units::new(500),
                    bytes: ByteCount::new(1000)
                }))
            ]
        })
    );
    assert_eq!(
        parse(&[
            "preview",
            "--input-tokens",
            "inherit",
            "--max-total-attempts",
            "inherit",
            "--max-transport-retries",
            "inherit",
            "--reasoning-effort",
            "inherit",
            "--retrieval-limits",
            "inherit"
        ]),
        Ok(Command::PreviewSelected {
            selected: vec![
                Edit::InputTokens(None),
                Edit::EscalationMaxTotalAttempts(None),
                Edit::EscalationMaxTransportRetries(None),
                Edit::ReasoningEffort(None),
                Edit::RetrievalLimits(None)
            ]
        })
    );
    for args in [
        vec!["preview", "--input-tokens", "0"],
        vec!["preview", "--max-transport-retries", "5"],
        vec!["preview", "--max-quality-switches", "9"],
        vec!["preview", "--max-total-attempts", "0"],
        vec!["preview", "--minimum-repeated-failures", "65"],
        vec!["preview", "--reasoning-effort", "unknown"],
        vec!["preview", "--retrieval-limits", "2", "500"],
        vec!["preview", "--retrieval-limits", "2", "1", "1000"],
        vec!["preview", "--input-tokens", "2", "--input-tokens", "3"],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[test]
fn selected_preview_shows_clamped_fields_and_applies_exactly_the_selection() {
    let selected = vec![Edit::AllowedModels(BTreeSet::from([
        "selected".into(),
        "outside".into(),
    ]))];
    let mut proposal = preview();
    proposal.selected = selected.clone();
    proposal.persisted.allowed_models = BTreeSet::from(["selected".into(), "outside".into()]);
    proposal.effective.allowed_models = BTreeSet::from(["selected".into()]);
    let mut session = Session {
        report: Some(report()),
        pending: None,
    };
    let message = session
        .execute(
            Command::PreviewSelected {
                selected: selected.clone(),
            },
            Timestamp::new(10),
            |request| {
                let Request::Preview {
                    selected: actual, ..
                } = request
                else {
                    panic!("preview only")
                };
                assert_eq!(actual, selected);
                Ok(serde_json::to_value(&proposal).unwrap())
            },
        )
        .unwrap();
    assert!(message
        .contains("allowed_models: [] -> [\"outside\",\"selected\"]; effective [\"selected\"]"));
    session
        .execute(Command::Apply, Timestamp::new(10), |request| {
            let Request::Apply { preview, command } = request else {
                panic!("explicit apply only")
            };
            assert_eq!(*preview, proposal);
            Ok(serde_json::to_value(receipt(command)).unwrap())
        })
        .unwrap();
    assert!(session.pending.is_none());
}

#[test]
fn malformed_terminal_replacement_cannot_apply_the_previous_preview() {
    let mut session = Session {
        report: Some(report()),
        pending: Some((CommandId::new(), preview())),
    };
    session.prepare_input("/optimize status");
    assert!(session.pending.is_some());
    let input = "  /optimize preview --models a,a";
    session.prepare_input(input);
    assert!(crate::terminal::parse(input).is_err());
    assert!(session
        .execute(Command::Apply, Timestamp::new(10), |_| panic!(
            "must not publish old preview"
        ))
        .is_err());
}
#[test]
fn report_displays_uncertainty_and_answers_use_durable_revision() {
    let mut session = Session::default();
    let mut calls = Vec::new();
    let text=session.execute(Command::Report,Timestamp::new(10),|request| {
        calls.push(request.clone());
        Ok(match request {
            Request::Status=>serde_json::json!({"policy":null,"interview":interview(),"next_question":"priority"}),
            Request::Report {from,until}=>{assert_eq!(from,None);assert_eq!(until,Timestamp::new(10));serde_json::json!({"report":report(),"interview":interview(),"next_question":"priority"})},
            _=>panic!("report must not change policy"),
        })
    }).unwrap();
    assert_eq!(calls.len(), 2);
    assert!(text.contains("uncertain attempts: 1"));
    assert!(text.contains("not configured"));
    assert!(text.contains("/optimize answer priority"));
    let mut saved = interview();
    saved.answers.insert(Question::Priority, "quality".into());
    let text = session
        .execute(
            Command::Answer {
                question: Question::Priority,
                value: "quality".into(),
            },
            Timestamp::new(10),
            |request| {
                Ok(match request {
                    Request::Status => serde_json::json!({"interview":interview()}),
                    Request::Answer {
                        expected,
                        question,
                        value,
                    } => {
                        assert_eq!(expected, None);
                        assert_eq!(question, Question::Priority);
                        assert_eq!(value, "quality");
                        serde_json::json!({"interview":saved,"next_question":"expected_size"})
                    }
                    _ => panic!("answer must not apply policy"),
                })
            },
        )
        .unwrap();
    assert!(text.contains("Project preference saved"));
    let next = Question::ModelRestrictions;
    session
        .execute(
            Command::Answer {
                question: next.clone(),
                value: "none".into(),
            },
            Timestamp::new(10),
            |request| {
                Ok(match request {
                    Request::Status => serde_json::json!({"interview":saved}),
                    Request::Answer {
                        expected, question, ..
                    } => {
                        assert_eq!(expected, Some(Revision::ZERO));
                        assert_eq!(question, next);
                        serde_json::json!({"interview":saved,"next_question":"review_preference"})
                    }
                    _ => panic!("answer must not schedule work"),
                })
            },
        )
        .unwrap();
}
#[test]
fn preview_contains_selected_ordering_and_effective_limits_then_apply_is_explicit() {
    let mut session = Session {
        report: Some(report()),
        pending: None,
    };
    let text = session
        .execute(
            Command::Preview {
                profile: Profile::High,
                quality_floor_bps: 9000,
            },
            Timestamp::new(10),
            |request| {
                let Request::Preview { report, selected } = request else {
                    panic!("preview called another operation")
                };
                assert_eq!(report, "report-fixture");
                assert_eq!(
                    selected,
                    vec![
                        Edit::Profile(Profile::High),
                        Edit::Ordering(vec![
                            Preference::Quality,
                            Preference::Capability,
                            Preference::Latency,
                            Preference::TotalCost
                        ]),
                        Edit::QualityFloorBps(9000)
                    ]
                );
                Ok(serde_json::to_value(preview()).unwrap())
            },
        )
        .unwrap();
    assert!(text.contains("quality floor 9000/10000"));
    assert!(text.contains("quality floor 9500/10000"));
    assert!(text.contains("/optimize apply"));
    let saved_command = session.pending.as_ref().unwrap().0.clone();
    let error = session.execute(Command::Apply, Timestamp::new(10), |request| {
        let Request::Apply {
            command,
            preview: proposal,
        } = request
        else {
            panic!("apply must carry stored preview")
        };
        assert_eq!(command, saved_command);
        assert_eq!(*proposal, preview());
        Err("reply unavailable after submission".into())
    });
    assert!(error.is_err());
    assert!(session.pending.is_some());
    session
        .execute(Command::Apply, Timestamp::new(10), |request| {
            let Request::Apply { command, .. } = request else {
                panic!("apply retry")
            };
            assert_eq!(command, saved_command);
            Ok(serde_json::to_value(receipt(command)).unwrap())
        })
        .unwrap();
    assert!(session.pending.is_none());
    assert!(session
        .execute(Command::Apply, Timestamp::new(10), |_| panic!(
            "no second apply without preview"
        ))
        .is_err());
}
#[test]
fn rejected_replacement_clears_old_preview_and_rollback_uses_explicit_revisions() {
    let mut session = Session {
        report: Some(report()),
        pending: Some((CommandId::new(), preview())),
    };
    assert!(session
        .execute(
            Command::Preview {
                profile: Profile::Low,
                quality_floor_bps: 9000
            },
            Timestamp::new(10),
            |_| Err("policy changed".into())
        )
        .is_err());
    assert!(session.pending.is_none());
    let text = session
        .execute(
            Command::Rollback {
                target: Revision::new(2),
                expected: Revision::new(5),
            },
            Timestamp::new(10),
            |request| {
                let Request::Rollback {
                    command,
                    expected,
                    target,
                } = request
                else {
                    panic!("rollback operation")
                };
                assert_eq!(expected, Revision::new(5));
                assert_eq!(target, Revision::new(2));
                Ok(serde_json::to_value(receipt(command)).unwrap())
            },
        )
        .unwrap();
    assert!(text.contains("from revision 2"));
}
#[test]
fn adaptive_questions_omit_irrelevant_gaps_but_manual_answers_remain_available() {
    let mut small = report();
    small.counts = Counts::default();
    small.cohorts.clear();
    let mut saved = interview();
    saved.answers.insert(Question::Priority, "cost".into());
    let response = serde_json::json!({"interview":saved,"next_question":"expected_size"});
    assert_eq!(next_question(&response, Some(&small)).unwrap(), None);
    assert_eq!(
        parse(&["answer", "restrictions", "exclude", "fixture/provider"]),
        Ok(Command::Answer {
            question: Question::ModelRestrictions,
            value: "exclude fixture/provider".into()
        })
    );
}

#[test]
fn group_inspection_is_paged_and_shows_unknowns_and_per_role_evidence() {
    use vcp_lifecycle::foundation::routing_state::Registry;
    use vcp_models::routing::{
        Candidate, CatalogRevision, EvidenceKind, Group, GroupMembership, ModelEndpoint,
        Provenance, RoleEvidence, State,
    };
    let source = Provenance {
        source: "fixture://nomination".into(),
        sha256: "a".repeat(64),
        observed_at: Timestamp::new(1),
        effective_at: None,
        limitations: vec!["Synthetic research only; no live qualification".into()],
    };
    let entries = (0..10)
        .map(|position| Candidate {
            identity: ModelEndpoint {
                model: format!("fixture/{position:02}"),
                endpoint: "fixture/provider".into(),
            },
            availability: State::Unknown,
            reasons: vec!["Qualification missing".into()],
            provenance: vec![source.clone()],
            capabilities: BTreeMap::new(),
            snapshot: None,
            compatibility: vec![],
            memberships: vec![GroupMembership {
                version: "research-v1".into(),
                group: Group::High,
                roles: vec![RoleEvidence {
                    id: "research-observation".into(),
                    role: vcp_domain::accounting::RequestRole::Main,
                    task_class: "coding".into(),
                    kind: EvidenceKind::Research,
                    observed_at: Timestamp::new(1),
                    valid_until: Timestamp::new(100),
                    samples: 0,
                    quality_bps: 0,
                    latency_p50_ms: 0,
                    latency_p95_ms: 0,
                    usage_p50: None,
                    usage_p95: None,
                    provenance: vec![source.clone()],
                }],
            }],
        })
        .collect();
    let catalog = CatalogRevision::create(None, Timestamp::new(2), None, entries).unwrap();
    let registry = Published {
        revision: Revision::ZERO,
        parent: None,
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        timestamp: Timestamp::new(2),
        value: Registry {
            catalog,
            raw: vcp_domain::ArtifactId::parse("raw-catalog").unwrap(),
            raw_sha256: "b".repeat(64),
        },
    };
    let response = serde_json::json!({"registry":registry});
    let page = groups::render(&response, None, 0, Timestamp::new(10)).unwrap();
    assert!(page.contains("fixture/00"));
    assert!(page.contains("fixture/07"));
    assert!(!page.contains("fixture/08 @"));
    assert!(page.contains("/groups --offset 8"));
    assert!(page.contains("Unknown"));
    assert!(page.contains("High / Main / coding: Research evidence"));
    assert!(page.contains("age 9 ms"));
    assert!(page.contains("Snapshot unavailable"));
    assert!(page.contains("fixture://nomination"));
    assert!(page.len() < 40_000);
    let filtered = groups::render(&response, Some("fixture/09"), 0, Timestamp::new(10)).unwrap();
    assert!(filtered.contains("1 matching candidates"));
    assert!(filtered.contains("fixture/09 @"));
    assert!(!filtered.contains("fixture/00 @"));
    assert_eq!(
        parse_groups(&["fixture/09", "--offset", "8"]),
        Ok(Command::Groups {
            model: Some("fixture/09".into()),
            offset: 8
        })
    );
    for invalid in [
        vec!["--offset", "-1"],
        vec!["--offset", "1025"],
        vec!["fixture/09", "extra"],
        vec!["--unknown"],
    ] {
        assert!(parse_groups(&invalid).is_err());
    }
    let mut session = Session::default();
    assert!(session
        .execute(
            Command::Groups {
                model: None,
                offset: 0
            },
            Timestamp::new(10),
            |request| {
                assert!(matches!(request, Request::Status));
                Ok(serde_json::json!({"registry":null}))
            }
        )
        .unwrap()
        .contains("not configured"));
}
