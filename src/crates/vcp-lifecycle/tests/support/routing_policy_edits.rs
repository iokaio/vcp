// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_models::routing::{ModelEndpoint, Pin};

fn endpoint(model: &str) -> ModelEndpoint {
    ModelEndpoint {
        model: model.into(),
        endpoint: "endpoint".into(),
    }
}

#[test]
fn resource_limits_clamp_each_direction_and_cannot_enable_escalation() {
    use vcp_models::routing::{EscalationLimits, RetrievalLimits};
    let original = policy();
    let mut selected = original.clone();
    selected.input_tokens = Some(Units::new(1000));
    selected.escalation_limits = Some(EscalationLimits {
        max_transport_retries: Some(4),
        max_quality_switches: Some(8),
        max_total_attempts: Some(64),
        minimum_repeated_failures: Some(1),
    });
    selected.retrieval_limits = Some(RetrievalLimits::default());
    selected = selected.seal().unwrap();
    assert!(
        effective_policy(selected.clone(), &original).is_err(),
        "selection cannot enable missing trusted escalation"
    );
    let mut ceiling = original;
    ceiling.input_tokens = Some(Units::new(100));
    ceiling.escalation_limits = Some(EscalationLimits {
        max_transport_retries: Some(1),
        max_quality_switches: Some(2),
        max_total_attempts: Some(3),
        minimum_repeated_failures: Some(4),
    });
    ceiling.retrieval_limits = Some(RetrievalLimits {
        results: 2,
        tokens: Units::new(500),
        bytes: ByteCount::new(1000),
    });
    ceiling = ceiling.seal().unwrap();
    let effective = effective_policy(selected, &ceiling).unwrap();
    assert_eq!(effective.input_tokens, Some(Units::new(100)));
    assert_eq!(effective.escalation_limits, ceiling.escalation_limits);
    assert_eq!(effective.retrieval_limits, ceiling.retrieval_limits);
}

#[tokio::test]
async fn selected_escalation_and_input_edits_apply_clear_and_rollback_under_current_ceilings() {
    use vcp_models::routing::EscalationLimits;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let initial = initialize_policy(&mut store, &access, policy(), Timestamp::new(2))
            .await
            .unwrap();
        let report = save_report(
            &mut store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(3),
            },
            Timestamp::new(3),
        )
        .await
        .unwrap();
        let mut ceilings = policy();
        ceilings.input_tokens = Some(Units::new(500));
        ceilings.reasoning_effort = Some(vcp_models::reasoning::Effort::Medium);
        ceilings.escalation_limits = Some(EscalationLimits {
            max_transport_retries: Some(2),
            max_quality_switches: Some(2),
            max_total_attempts: Some(4),
            minimum_repeated_failures: Some(1),
        });
        ceilings = ceilings.seal().unwrap();
        let selected = preview(
            &store,
            &access,
            &report.id,
            vec![
                Edit::InputTokens(Some(Units::new(900))),
                Edit::EscalationMaxQualitySwitches(Some(0)),
                Edit::ReasoningEffort(Some(vcp_models::reasoning::Effort::Low)),
            ],
            &ceilings,
        )
        .unwrap();
        assert_eq!(selected.persisted.input_tokens, Some(Units::new(900)));
        assert_eq!(selected.effective.input_tokens, Some(Units::new(500)));
        assert_eq!(
            selected
                .effective
                .escalation_limits
                .as_ref()
                .unwrap()
                .max_quality_switches,
            Some(0)
        );
        let applied = apply(
            &mut store,
            &access,
            CommandId::new(),
            &selected,
            &ceilings,
            Timestamp::new(4),
        )
        .await
        .unwrap();
        let cleared = preview(
            &store,
            &access,
            &report.id,
            vec![
                Edit::EscalationMaxQualitySwitches(None),
                Edit::ReasoningEffort(None),
            ],
            &ceilings,
        )
        .unwrap();
        assert!(cleared.persisted.escalation_limits.is_none());
        assert!(cleared.persisted.reasoning_effort.is_none());
        assert_eq!(
            cleared.effective.reasoning_effort,
            Some(vcp_models::reasoning::Effort::Medium)
        );
        assert_eq!(
            cleared
                .effective
                .escalation_limits
                .as_ref()
                .unwrap()
                .max_quality_switches,
            Some(2)
        );
        ceilings.input_tokens = Some(Units::new(100));
        ceilings = ceilings.seal().unwrap();
        let restored = rollback(
            &mut store,
            &access,
            CommandId::new(),
            applied.published.revision,
            initial.revision,
            &ceilings,
            Timestamp::new(5),
        )
        .await
        .unwrap();
        assert_eq!(restored.effective.input_tokens, Some(Units::new(100)));
        store.close().await.unwrap();
    }
}

#[test]
fn selected_output_inherits_and_clamps_without_zero_or_digest_loss() {
    let original = policy();
    let legacy = serde_json::to_value(&original).unwrap();
    assert!(legacy.get("output_tokens").is_none());
    let mut ceiling = original.clone();
    ceiling.output_tokens = Some(Units::new(256));
    ceiling = ceiling.seal().unwrap();
    assert_eq!(
        effective_policy(original.clone(), &ceiling)
            .unwrap()
            .output_tokens,
        Some(Units::new(256))
    );
    let mut selected = original.clone();
    selected.output_tokens = Some(Units::new(1024));
    selected = selected.seal().unwrap();
    assert_eq!(
        effective_policy(selected.clone(), &ceiling)
            .unwrap()
            .output_tokens,
        Some(Units::new(256))
    );
    selected.output_tokens = Some(Units::new(64));
    selected = selected.seal().unwrap();
    assert_eq!(
        effective_policy(selected, &ceiling).unwrap().output_tokens,
        Some(Units::new(64))
    );
    let decoded: Policy = serde_json::from_value(legacy).unwrap();
    assert_eq!(decoded.id, original.id);
}

#[tokio::test]
async fn selected_permissions_are_bounded_atomic_and_replayable_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let mut ceilings = policy();
        ceilings.allowed_models.insert("second".into());
        ceilings.allowed_groups.insert(Group::High);
        ceilings = ceilings.seal().unwrap();
        initialize_policy(&mut store, &access, ceilings.clone(), Timestamp::new(2))
            .await
            .unwrap();
        let report = save_report(
            &mut store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(3),
            },
            Timestamp::new(3),
        )
        .await
        .unwrap();
        let pin = Pin {
            candidate: endpoint("second"),
            fallback_candidates: BTreeSet::new(),
        };
        let proposal = preview(
            &store,
            &access,
            &report.id,
            vec![
                Edit::AllowedModels(BTreeSet::from(["second".into(), "untrusted".into()])),
                Edit::AllowedEndpoints(BTreeSet::from(["endpoint".into(), "untrusted".into()])),
                Edit::AllowedGroups(BTreeSet::from([Group::High, Group::Frontier])),
                Edit::Pin(Some(pin.clone())),
            ],
            &ceilings,
        )
        .unwrap();
        assert!(proposal.persisted.allowed_models.contains("untrusted"));
        assert_eq!(
            proposal.effective.allowed_models,
            BTreeSet::from(["second".into()])
        );
        assert_eq!(
            proposal.effective.allowed_endpoints,
            BTreeSet::from(["endpoint".into()])
        );
        assert_eq!(
            proposal.effective.allowed_groups,
            BTreeSet::from([Group::High])
        );
        assert_eq!(proposal.effective.pin, Some(pin));
        assert_eq!(
            proposal.persisted.quality_floor_bps,
            ceilings.quality_floor_bps
        );
        assert_eq!(proposal.persisted.ordering, ceilings.ordering);
        assert_eq!(
            proposal.persisted.deny_data_collection,
            ceilings.deny_data_collection
        );
        assert_eq!(proposal.persisted.require_zdr, ceilings.require_zdr);

        // A competing valid preview from the same base cannot overwrite the winner.
        let competing = preview(
            &store,
            &access,
            &report.id,
            vec![Edit::AllowedGroups(BTreeSet::new())],
            &ceilings,
        )
        .unwrap();
        let command = CommandId::new();
        let receipt = apply(
            &mut store,
            &access,
            command.clone(),
            &proposal,
            &ceilings,
            Timestamp::new(4),
        )
        .await
        .unwrap();
        let watermark = store.state().watermark;
        assert!(apply(
            &mut store,
            &access,
            CommandId::new(),
            &competing,
            &ceilings,
            Timestamp::new(5)
        )
        .await
        .is_err());
        assert!(apply(
            &mut store,
            &access,
            command.clone(),
            &competing,
            &ceilings,
            Timestamp::new(5)
        )
        .await
        .is_err());
        assert_eq!(store.state().watermark, watermark);
        store.close().await.unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            apply(
                &mut store,
                &access,
                command,
                &proposal,
                &ceilings,
                Timestamp::new(6)
            )
            .await
            .unwrap(),
            receipt
        );
        assert_eq!(
            current_policy(&store, &access).unwrap().unwrap(),
            receipt.published
        );

        // Rollback restores historical preferences but recomputes current ceilings.
        let mut narrowed = policy();
        narrowed.pin = Some(Pin {
            candidate: endpoint("model"),
            fallback_candidates: BTreeSet::new(),
        });
        narrowed = narrowed.seal().unwrap();
        let restored = rollback(
            &mut store,
            &access,
            CommandId::new(),
            receipt.published.revision,
            Revision::ZERO,
            &narrowed,
            Timestamp::new(7),
        )
        .await
        .unwrap();
        assert!(restored.published.value.allowed_models.contains("second"));
        assert_eq!(
            restored.effective.allowed_models,
            BTreeSet::from(["model".into()])
        );
        assert_eq!(
            restored.effective.allowed_groups,
            BTreeSet::from([Group::Low])
        );
        assert_eq!(restored.effective.pin, narrowed.pin);

        let clear_pin = preview(
            &store,
            &access,
            &report.id,
            vec![Edit::Pin(None)],
            &narrowed,
        )
        .unwrap();
        assert_eq!(clear_pin.persisted.pin, None);
        assert_eq!(clear_pin.effective.pin, narrowed.pin);
        let conflict = preview(
            &store,
            &access,
            &report.id,
            vec![Edit::Pin(Some(Pin {
                candidate: endpoint("second"),
                fallback_candidates: BTreeSet::new(),
            }))],
            &narrowed,
        )
        .unwrap();
        assert!(conflict.effective.allowed_models.is_empty());
        assert_eq!(conflict.effective.pin, narrowed.pin);

        // Changed trusted ceilings invalidate a prepared apply, even if its base is current.
        let watermark = store.state().watermark;
        assert!(apply(
            &mut store,
            &access,
            CommandId::new(),
            &clear_pin,
            &ceilings,
            Timestamp::new(8)
        )
        .await
        .is_err());
        let mut tampered = clear_pin.clone();
        tampered.effective.allowed_models.insert("untrusted".into());
        assert!(apply(
            &mut store,
            &access,
            CommandId::new(),
            &tampered,
            &narrowed,
            Timestamp::new(8)
        )
        .await
        .is_err());
        assert_eq!(store.state().watermark, watermark);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn invalid_or_duplicate_permission_edits_never_publish() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let ceilings = policy();
        initialize_policy(&mut store, &access, ceilings.clone(), Timestamp::new(2))
            .await
            .unwrap();
        let report = save_report(
            &mut store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(3),
            },
            Timestamp::new(3),
        )
        .await
        .unwrap();
        let watermark = store.state().watermark;
        for edits in [
            vec![Edit::AllowedModels(BTreeSet::from(["".into()]))],
            vec![Edit::AllowedEndpoints(BTreeSet::from([
                "bad\nendpoint".into()
            ]))],
            vec![Edit::AllowedModels(
                (0..1025).map(|n| format!("model-{n}")).collect(),
            )],
            vec![Edit::AllowedEndpoints(
                (0..1025).map(|n| format!("endpoint-{n}")).collect(),
            )],
            vec![
                Edit::AllowedGroups(BTreeSet::new()),
                Edit::AllowedGroups(BTreeSet::from([Group::Low])),
            ],
            vec![Edit::Pin(None), Edit::Pin(None)],
            vec![Edit::Pin(Some(Pin {
                candidate: endpoint(""),
                fallback_candidates: BTreeSet::new(),
            }))],
            vec![Edit::Pin(Some(Pin {
                candidate: endpoint("model"),
                fallback_candidates: BTreeSet::from([endpoint("model")]),
            }))],
        ] {
            assert!(preview(&store, &access, &report.id, edits, &ceilings).is_err());
        }
        for malformed in [
            serde_json::json!({"field":"allowed_models","value":"model"}),
            serde_json::json!({"field":"allowed_groups","value":["unrestricted"]}),
            serde_json::json!({"field":"pin","value":{"candidate":{"model":"model","endpoint":"endpoint"},"fallback_candidates":[],"grant":true}}),
            serde_json::json!({"field":"allowed_models","value":[],"grant":true}),
            serde_json::json!({"field":"require_zdr","value":false}),
        ] {
            assert!(serde_json::from_value::<Edit>(malformed).is_err());
        }
        let empty = preview(
            &store,
            &access,
            &report.id,
            vec![Edit::AllowedModels(BTreeSet::new())],
            &ceilings,
        )
        .unwrap();
        assert!(empty.effective.allowed_models.is_empty());
        assert_eq!(store.state().watermark, watermark);
        store.close().await.unwrap();
    }
}
