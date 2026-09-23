// SPDX-License-Identifier: Apache-2.0
//! P8 U05/U07: offline optimizer writes cannot silently apply stale retention.
//! Included by memory_local.rs; uses its paused, governed two-task fixture.
use super::*;

async fn state(fixture: &Fixture) -> State {
    let store = Store::open(&fixture.config.canonical_root, fixture.config.backend, &[])
        .await
        .unwrap();
    let result = store.state().clone();
    store.close().await.unwrap();
    result
}

fn refused(output: Output, diagnostic: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(diagnostic), "{stderr}");
}

fn assert_interview_record(snapshot: &State, expected_revision: u64, answers: Value) {
    // Derive the canonical key independently of the CLI status/answer response.
    let digest = vcp_protocol::digest_bytes(b"interview:workspace");
    let id = format!("routing-interview-{}", &digest[..32]);
    let record = snapshot
        .record(
            Collection::Projection,
            &id,
            &WorkspaceId::parse("workspace").unwrap(),
        )
        .unwrap();
    assert_eq!(record.revision, Revision::new(expected_revision));
    assert_eq!(
        record.value["revision"],
        json!(Revision::new(expected_revision))
    );
    assert_eq!(record.value["answers"], answers);
}

#[tokio::test]
async fn production_memory_optimizer_retention_are_offline_and_revision_bound() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let missing = fixture.data.join("missing-assets");
        let missing = missing.to_str().unwrap();
        let built = success(fixture.call(&[
            "memory",
            "build",
            "--assets",
            missing,
            "--allow-lexical-only",
        ]));
        assert_eq!(built["status"], "lexical_only");
        assert_scoped(&fixture.assert_search_is_read_only().await);

        // Freeze the same observed time window for both reports. No attempt
        // cohorts are fabricated just to make their comparison look useful.
        let until = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string();
        let before =
            success(fixture.call(&["optimize", "report", "--from", "0", "--until", &until]));
        let baseline = before["report"]["id"].as_str().unwrap();
        assert_eq!(before["report"]["counts"]["attempts"], 0);
        let answered = success(fixture.call(&[
            "optimize",
            "answer",
            "priority",
            "correctness",
            "--expected-revision",
            "0",
        ]));
        // The first persistent record starts at revision zero; its next update
        // increments to one. A zero expected revision alone is not yet stale.
        assert_eq!(answered["interview"]["revision"], json!(Revision::ZERO));
        assert_interview_record(&state(&fixture).await, 0, json!({"priority":"correctness"}));

        let preview = success(fixture.call(&[
            "memory",
            "prune",
            "--task",
            "task",
            "--preview",
            "--action",
            "exclude",
        ]));
        assert!(preview["selected_count"].as_u64().unwrap() > 0);
        let answered = success(fixture.call(&[
            "optimize",
            "answer",
            "review",
            "required",
            "--expected-revision",
            "0",
        ]));
        assert_eq!(answered["interview"]["revision"], json!(Revision::new(1)));
        let protected = state(&fixture).await;
        assert_interview_record(
            &protected,
            1,
            json!({"priority":"correctness","review_preference":"required"}),
        );

        refused(
            fixture.call(&[
                "optimize",
                "answer",
                "size",
                "small",
                "--expected-revision",
                "0",
            ]),
            "optimization interview changed",
        );
        assert_eq!(
            state(&fixture).await,
            protected,
            "stale answer mutated canonical state"
        );
        refused(
            fixture.call(&["prune", "apply", preview["id"].as_str().unwrap()]),
            "stale preview",
        );
        assert_eq!(
            state(&fixture).await,
            protected,
            "stale prune mutated canonical state"
        );

        let status = success(fixture.call(&["optimize", "status"]));
        assert_eq!(status["interview"]["revision"], json!(Revision::new(1)));
        assert_eq!(
            status["interview"]["answers"],
            json!({"priority":"correctness","review_preference":"required"})
        );
        assert_eq!(
            state(&fixture).await,
            protected,
            "status changed canonical state"
        );
        let after =
            success(fixture.call(&["optimize", "report", "--from", "0", "--until", &until]));
        let current = after["report"]["id"].as_str().unwrap();
        assert_ne!(baseline, current);
        assert_eq!(after["report"]["counts"]["attempts"], 0);
        let after_report = state(&fixture).await;
        let compared = success(fixture.call(&["optimize", "compare", baseline, current]));
        assert_eq!(compared["comparable"], false, "{compared}");
        assert_eq!(compared["automatic_action"], false);
        assert!(compared["metrics"].as_array().unwrap().is_empty());
        assert!(
            compared["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason
                    .as_str()
                    .is_some_and(|text| text.contains("Empty task or declared attempt cohort"))),
            "{compared}"
        );
        assert_eq!(
            state(&fixture).await,
            after_report,
            "comparison changed canonical state"
        );
        assert_scoped(&fixture.assert_search_is_read_only().await);
        fixture.assert_no_dispatch_or_resume().await;

        // A fresh explicit exclusion must work, while old memory/optimizer
        // operations must not resurrect the selected claim on reopening.
        let fresh = success(fixture.call(&[
            "memory",
            "prune",
            "--task",
            "task",
            "--preview",
            "--action",
            "exclude",
        ]));
        assert!(fresh["selected_count"].as_u64().unwrap() > 0);
        assert_ne!(fresh["id"], preview["id"]);
        let applied = success(fixture.call(&["prune", "apply", fresh["id"].as_str().unwrap()]));
        assert_eq!(applied["preview"]["id"], fresh["id"]);
        let excluded_state = state(&fixture).await;
        let excluded = success(fixture.call(&["memory", "search", "retained", "--task", "task"]));
        assert!(
            excluded["passages"].as_array().unwrap().is_empty(),
            "{excluded}"
        );
        assert_eq!(
            state(&fixture).await,
            excluded_state,
            "post-exclusion search changed state"
        );

        let rebuilt = success(fixture.call(&[
            "memory",
            "build",
            "--assets",
            missing,
            "--allow-lexical-only",
        ]));
        assert_eq!(rebuilt["status"], "lexical_only");
        let excluded = success(fixture.call(&["memory", "search", "retained", "--task", "task"]));
        assert!(excluded["passages"].as_array().unwrap().is_empty());
        let sibling = success(fixture.call(&[
            "memory",
            "search",
            "retained ocean submarine",
            "--task",
            "other-task",
        ]));
        let passages = sibling["passages"].as_array().unwrap();
        assert!(!passages.is_empty(), "unselected sibling lost: {sibling}");
        assert!(passages
            .iter()
            .all(|passage| passage["scope"]["task"] == "other-task"));
        assert_interview_record(
            &state(&fixture).await,
            1,
            json!({"priority":"correctness","review_preference":"required"}),
        );
        fixture.assert_no_dispatch_or_resume().await;
    }
}
