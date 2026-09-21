// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::task::TaskState;
use vcp_lifecycle::foundation::routing_state::advisory::{self, Disposition};
use vcp_models::decision::{
    self, Answer, Binding as DecisionBinding, Mode, Operation, Outcome, Purpose,
    QualifiedEvaluator, Question, Request, Usage,
};
use vcp_protocol::{canonical_bytes, digest_bytes};

fn prepare(binding: DecisionBinding, marker: &str) -> decision::Prepared {
    let state = serde_json::json!({"marker":marker,"observations":["bounded"]});
    let request = Request {
        version: decision::VERSION,
        binding: DecisionBinding {
            input: digest_bytes(&canonical_bytes(&state).unwrap()),
            ..binding
        },
        purpose: Purpose::Escalation,
        question_revision: digest_bytes(b"advisory-record-test-v1"),
        state,
        questions: BTreeMap::from([(
            "next_action".into(),
            Question::Choice {
                instructions: "Choose a bounded action from recorded evidence.".into(),
                options: BTreeMap::from([
                    ("retry".into(), "Retry under the same limits".into()),
                    ("stop".into(), "Stop the bounded loop".into()),
                ]),
            },
        )]),
        deadline: Timestamp::new(100),
    };
    let evaluator = QualifiedEvaluator {
        model: "fixture/advisory".into(),
        provider: "fixture".into(),
        served_model: "fixture/advisory-v1".into(),
        served_provider: "Fixture".into(),
        operation: Operation::JevDecisions,
        purpose: Purpose::Escalation,
        mode: Mode::Advisory,
        evidence_digest: "a".repeat(64),
        configuration_digest: "b".repeat(64),
        valid_until: Timestamp::new(200),
        require_distributions: true,
        require_confidence: true,
        deny_data_collection: true,
        require_zdr: true,
        prompt_price_per_million: "0.01".into(),
        output_price_per_million: "0.01".into(),
        request_price: "0".into(),
    };
    decision::prepare(
        &request,
        &decision::Policy {
            mode: Mode::Advisory,
            evaluator: Some(evaluator),
            attempt_limit: 1,
            attempts_used: 0,
        },
        Timestamp::new(10),
    )
    .unwrap()
    .unwrap()
}

fn advice(prepared: &decision::Prepared, action: &str) -> Outcome {
    Outcome::Advice {
        binding: prepared.request().binding.clone(),
        purpose: Purpose::Escalation,
        question_revision: prepared.request().question_revision.clone(),
        request_digest: prepared.digest().into(),
        evaluator: prepared.evaluator().clone(),
        mode: Mode::Advisory,
        answers: BTreeMap::from([(
            "next_action".into(),
            Answer::Choice {
                choice: action.into(),
                probabilities: Some(BTreeMap::from([
                    ("retry".into(), if action == "retry" { 0.9 } else { 0.1 }),
                    ("stop".into(), if action == "stop" { 0.9 } else { 0.1 }),
                ])),
                confidence: Some(0.9),
            },
        )]),
        usage: Usage {
            input_tokens: Some(10),
            output_tokens: Some(2),
            observed_cost: Some(Micros::new(1)),
            unknown_liability: false,
        },
    }
}

#[tokio::test]
async fn canonical_advisory_records_deduplicate_revalidate_and_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task =
            super::action_evidence::create_task(&mut store, &access, TaskState::Running).await;
        let base = DecisionBinding {
            scope: task.scope.clone(),
            root: task.root.clone(),
            step: task.revision,
            steering: task.steering,
            authority: access.authority,
            deletion: DeletionEpoch::ZERO,
            policy: "c".repeat(64),
            catalog: "d".repeat(64),
            input: String::new(),
            evidence: BTreeMap::from([("observation".into(), "e".repeat(64))]),
        };

        let prepared = prepare(base.clone(), "current");
        let current = prepared.request().binding.clone();
        let request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &prepared,
            &current,
            Timestamp::new(10),
        )
        .await
        .unwrap();
        assert_eq!(request.request_digest.len(), 64);
        assert_eq!(request.transport_commitment, prepared.digest());
        let watermark = store.state().watermark;
        assert_eq!(
            advisory::record_request(
                &mut store,
                &access,
                CommandId::new(),
                &prepared,
                &current,
                Timestamp::new(11),
            )
            .await
            .unwrap()
            .id,
            request.id
        );
        assert_eq!(store.state().watermark, watermark);

        let outcome = advice(&prepared, "retry");
        let result = advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &request.id,
            &outcome,
            &current,
            Timestamp::new(20),
        )
        .await
        .unwrap();
        assert_eq!(result.disposition, Disposition::AcceptedCurrent);
        let result_watermark = store.state().watermark;
        assert_eq!(
            advisory::record_result(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                &outcome,
                &current,
                Timestamp::new(21),
            )
            .await
            .unwrap(),
            result
        );
        assert_eq!(store.state().watermark, result_watermark);
        assert!(advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &request.id,
            &Outcome::Baseline { reason: "conflict" },
            &current,
            Timestamp::new(22),
        )
        .await
        .unwrap_err()
        .contains("reused"));

        let late_prepared = prepare(base, "late");
        let late_current = late_prepared.request().binding.clone();
        let late_request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &late_prepared,
            &late_current,
            Timestamp::new(30),
        )
        .await
        .unwrap();
        let mut changed = late_current.clone();
        changed.policy = "f".repeat(64);
        let late_result = advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &late_request.id,
            &advice(&late_prepared, "stop"),
            &changed,
            Timestamp::new(40),
        )
        .await
        .unwrap();
        assert_eq!(late_result.disposition, Disposition::HistoricalStale);
        assert!(advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &late_prepared,
            &changed,
            Timestamp::new(41),
        )
        .await
        .unwrap_err()
        .contains("no longer current"));

        let denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::new()),
        };
        assert!(advisory::load_request(&store, &denied, &request.id).is_err());
        assert!(advisory::load_result(&store, &denied, &request.id).is_err());
        store.close().await.unwrap();

        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            advisory::load_request(&reopened, &access, &request.id)
                .unwrap()
                .transport_commitment,
            request.transport_commitment
        );
        assert_eq!(
            advisory::load_result(&reopened, &access, &request.id).unwrap(),
            result
        );
        assert_eq!(
            advisory::load_result(&reopened, &access, &late_request.id)
                .unwrap()
                .disposition,
            Disposition::HistoricalStale
        );
    }
}
