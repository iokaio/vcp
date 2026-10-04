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
