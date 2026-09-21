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
