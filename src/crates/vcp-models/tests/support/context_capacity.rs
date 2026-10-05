// SPDX-License-Identifier: Apache-2.0
//! Self-contained capacity regression through the production request codec.
use super::*;
use vcp_context::{compaction, manifest::*};
use vcp_domain::{artifact::*, workspace::Scope, *};

fn revisions() -> Revisions {
    Revisions {
        scope: Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        steering: SteeringRevision::ZERO,
        policy: PolicyRevision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        binding: Revision::ZERO,
        instructions: Revision::ZERO,
        tools: Revision::ZERO,
        skills: Revision::ZERO,
        memory: Revision::ZERO,
        task_state: Revision::ZERO,
    }
}

fn capture(scope: &Scope, bytes: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor {
        spec: ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "application/json".into(),
            schema: "synthetic-capacity/1".into(),
            source: "public-synthetic".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        },
        state: CaptureState::Complete,
        length: ByteCount::new(bytes.len() as u64),
        sha256: vcp_protocol::digest_bytes(bytes),
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::new(bytes.len() as u64),
        }],
    }
}

fn foundations(revisions: &Revisions) -> (Vec<Part>, Vec<ArtifactDescriptor>) {
    let mut descriptors = Vec::new();
    let parts = [
        (Kind::Operating, Trust::Operating),
        (Kind::Objective, Trust::User),
        (Kind::TaskState, Trust::Observed),
    ]
    .into_iter()
    .map(|(kind, trust)| {
        let descriptor = capture(&revisions.scope, b"required fixture");
        let part = Part::captured_text(
            descriptor.spec.id.to_string(),
            kind,
            trust,
            &descriptor,
            b"required fixture",
            true,
            0,
            "fixture".into(),
        )
        .unwrap();
        descriptors.push(descriptor);
        part
    })
    .collect();
    (parts, descriptors)
}

#[test]
fn encoding_work_preserves_codec_bytes_errors_and_sealed_short_circuit_order() {
    let snapshot = Snapshot::from_endpoints(
        &serde_json::to_vec(&catalog()).unwrap(),
        Timestamp::new(10),
        Timestamp::new(1000),
        compat(),
    )
    .unwrap();
    let env = super::envelope(
        &snapshot,
        Units::new(100),
        Units::new(512),
        Timestamp::new(30),
    )
    .unwrap();
    let schemas = tools();
    let mut work = EncodingWork::default();
    let expected = encode(&[], &env, &schemas, &snapshot).unwrap();
    assert_eq!(
        encode_with_effort_observed(&[], &env, &schemas, &snapshot, None, &mut work).unwrap(),
        expected
    );
    assert_eq!(
        (work.encode_calls, work.encode_failures, work.encoded_bytes),
        (1, 0, expected.len() as u64)
    );
    let mut stale = env.clone();
    stale.catalog = "wrong catalog".into();
    for (env, schemas) in [(&stale, json!(null)), (&env, json!(null))] {
        let plain = encode(&[], env, &schemas, &snapshot).unwrap_err();
        let observed = encode_with_effort_observed(&[], env, &schemas, &snapshot, None, &mut work)
            .unwrap_err();
        assert_eq!(format!("{plain:?}"), format!("{observed:?}"));
    }
    assert_eq!(
        (work.encode_calls, work.encode_failures, work.encoded_bytes),
        (3, 2, expected.len() as u64)
    );
    let revisions = revisions();
    let (parts, descriptors) = foundations(&revisions);
    let sealed = vcp_context::selection::assemble(
        parts,
        revisions,
        env,
        schemas.clone(),
        vec![],
        &vcp_context::selection::Utf8ByteCeiling,
        |parts, env, schemas| {
            encode(parts, env, schemas, &snapshot)
                .map_err(|_| vcp_context::manifest::Error::Incompatible("fixture codec"))
        },
    )
    .unwrap();
    let verified = sealed
        .verify_captures(|_, id, _| {
            Ok((
                descriptors
                    .iter()
                    .find(|d| d.spec.id == *id)
                    .unwrap()
                    .clone(),
                b"required fixture".to_vec(),
            ))
        })
        .unwrap();
    let mut work = EncodingWork::default();
    validate_sealed_with_effort_observed(
        &verified,
        &snapshot,
        &schemas,
        Timestamp::new(30),
        None,
        &mut work,
    )
    .unwrap();
    assert_eq!(
        (
            work.validation_calls,
            work.validation_failures,
            work.encode_calls
        ),
        (1, 0, 1)
    );
    assert!(work.validation_micros >= work.encode_micros);
    let mut changed = snapshot.clone();
    changed.id = "different admitted catalog".into();
    let mut failed = EncodingWork::default();
    let plain = validate_sealed(&verified, &changed, &schemas, Timestamp::new(30)).unwrap_err();
    let observed = validate_sealed_with_effort_observed(
        &verified,
        &changed,
        &schemas,
        Timestamp::new(30),
        None,
        &mut failed,
    )
    .unwrap_err();
    assert_eq!(format!("{plain:?}"), format!("{observed:?}"));
    assert_eq!(
        (
            failed.validation_calls,
            failed.validation_failures,
            failed.encode_calls,
            failed.encode_failures
        ),
        (1, 1, 1, 1)
    );
    for (now, schemas) in [
        (Timestamp::new(1001), json!(null)),
        (Timestamp::new(30), json!(null)),
    ] {
        let mut failed = EncodingWork::default();
        let plain = validate_sealed(&verified, &snapshot, &schemas, now).unwrap_err();
        let observed = validate_sealed_with_effort_observed(
            &verified,
            &snapshot,
            &schemas,
            now,
            None,
            &mut failed,
        )
        .unwrap_err();
        assert_eq!(format!("{plain:?}"), format!("{observed:?}"));
        assert_eq!(
            (
                failed.validation_calls,
                failed.validation_failures,
                failed.encode_calls
            ),
            (1, 1, 0)
        );
    }
}

#[test]
fn encoding_work_counts_actual_optional_selection_trials_without_changing_manifest() {
    let mut source = catalog();
    source["data"]["endpoints"][0]["context_length"] = json!(200_000);
    source["data"]["endpoints"][0]["max_prompt_tokens"] = json!(190_000);
    let snapshot = Snapshot::from_endpoints(
        &serde_json::to_vec(&source).unwrap(),
        Timestamp::new(10),
        Timestamp::new(1000),
        compat(),
    )
    .unwrap();
    let revisions = revisions();
    let env = super::envelope(
        &snapshot,
        Units::new(100),
        Units::new(512),
        Timestamp::new(30),
    )
    .unwrap();
    let mut parts = foundations(&revisions).0;
    parts.extend(
        (0..3)
            .map(|n| {
                let bytes = format!("optional fixture {n}").into_bytes();
                let descriptor = capture(&revisions.scope, &bytes);
                Part::captured_text(
                    descriptor.spec.id.to_string(),
                    Kind::Evidence,
                    Trust::Untrusted,
                    &descriptor,
                    &bytes,
                    false,
                    n,
                    "fixture".into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>(),
    );
    let work = std::cell::RefCell::new(EncodingWork::default());
    let observed = vcp_context::selection::assemble(
        parts.clone(),
        revisions.clone(),
        env.clone(),
        tools(),
        vec![],
        &vcp_context::selection::Utf8ByteCeiling,
        |parts, env, schemas| {
            encode_with_effort_observed(
                parts,
                env,
                schemas,
                &snapshot,
                None,
                &mut work.borrow_mut(),
            )
            .map_err(|_| vcp_context::manifest::Error::Incompatible("fixture codec"))
        },
    )
    .unwrap();
    let plain = vcp_context::selection::assemble(
        parts,
        revisions,
        env,
        tools(),
        vec![],
        &vcp_context::selection::Utf8ByteCeiling,
        |parts, env, schemas| {
            encode(parts, env, schemas, &snapshot)
                .map_err(|_| vcp_context::manifest::Error::Incompatible("fixture codec"))
        },
    )
    .unwrap();
    assert_eq!(observed.body(), plain.body());
    assert_eq!(observed.manifest_digest(), plain.manifest_digest());
    let work = work.into_inner();
    assert_eq!((work.encode_calls, work.encode_failures), (5, 0)); // mandatory, 3 optional trials, final
    assert!(work.encoded_bytes > observed.body().len() as u64);
}

#[test]
fn verbose_recent_pair_fits_both_history_windows_using_actual_codec() {
    let revisions = revisions();
    let mut history = Vec::new();
    for n in 0..14 {
        for content in [
            Content::ToolCall {
                id: format!("call-{n}"),
                name: "read_file".into(),
                arguments: json!({"path":"fixture.rs"}),
            },
            Content::ToolResult {
                id: format!("call-{n}"),
                output: if n == 13 {
                    "é🦀 quoted \"output\"\\\n".repeat(8000)
                } else {
                    format!("observed range {n}: {}", "content ".repeat(900))
                },
            },
        ] {
            let bytes = content.bytes().unwrap();
            let descriptor = capture(&revisions.scope, &bytes);
            let kind = if matches!(content, Content::ToolCall { .. }) {
                Kind::ToolCall
            } else {
                Kind::ToolResult
            };
            let mut part = Part::captured_text(
                descriptor.spec.id.to_string(),
                kind,
                Trust::Untrusted,
                &descriptor,
                &bytes,
                true,
                0,
                "synthetic tool evidence".into(),
            )
            .unwrap();
            part.content = content;
            history.push(part);
        }
    }
    let original = history.clone();
    let mut source = catalog();
    source["data"]["endpoints"][0]["context_length"] = json!(200_000);
    source["data"]["endpoints"][0]["max_prompt_tokens"] = json!(190_000);
    let snapshot = Snapshot::from_endpoints(
        &serde_json::to_vec(&source).unwrap(),
        Timestamp::new(10),
        Timestamp::new(1000),
        compat(),
    )
    .unwrap();
    let envelope = super::envelope(
        &snapshot,
        Units::new(8000),
        Units::new(512),
        Timestamp::new(30),
    )
    .unwrap();
    let before = encode(&history, &envelope, &tools(), &snapshot).unwrap();
    assert!(before.len() as u64 > snapshot.max_input.get());
    for keep_recent_pairs in [6, 12] {
        let projection = compaction::compact(
            &history,
            &revisions,
            &compaction::Config {
                keep_recent_pairs,
                preview_bytes: 512,
                minimum_gain_bytes: 2048,
            },
        )
        .unwrap()
        .unwrap();
        let verified = projection
            .revalidate(&revisions, &history, |id| {
                Ok(history
                    .iter()
                    .find(|p| &p.artifact == id)
                    .unwrap()
                    .content
                    .bytes()
                    .unwrap())
            })
            .unwrap();
        let descriptor = capture(&revisions.scope, projection.summary.as_bytes());
        let summary = verified.captured_part(&descriptor).unwrap();
        assert_eq!(summary.trust, Trust::Untrusted);
        let mut parts = vec![summary];
        parts.extend(projection.retained.clone());
        let body = encode(&parts, &envelope, &tools(), &snapshot).unwrap();
        assert!(body.len() as u64 <= snapshot.max_input.get());
        assert!(
            body.len() as u64 + envelope.output.get() + envelope.margin.get()
                < envelope.context.get()
        );
        assert_eq!(projection.sources.len(), history.len());
        assert!(!projection.retained.iter().any(|p| p.id == history[27].id));
        let wire: Value = serde_json::from_slice(&body).unwrap();
        let calls: Vec<_> = wire["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["type"] == "function_call")
            .collect();
        let outputs: Vec<_> = wire["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["type"] == "function_call_output")
            .collect();
        assert_eq!(calls.len(), outputs.len());
        for (call, output) in calls.iter().zip(outputs) {
            assert_eq!(call["call_id"], output["call_id"]);
        }
        for changed in [&history[1].artifact, &history[27].artifact] {
            assert!(projection
                .revalidate(&revisions, &history, |id| {
                    if id == changed {
                        Ok(b"tampered omitted evidence".to_vec())
                    } else {
                        Ok(history
                            .iter()
                            .find(|p| &p.artifact == id)
                            .unwrap()
                            .content
                            .bytes()
                            .unwrap())
                    }
                })
                .is_err());
        }
    }
    assert_eq!(history, original);
}
