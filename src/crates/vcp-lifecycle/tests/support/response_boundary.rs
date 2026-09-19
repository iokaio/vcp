// SPDX-License-Identifier: Apache-2.0
use super::*;
use codex_extension_api::*;
use core_test_support::streaming_sse::{start_streaming_sse_server, StreamingSseChunk};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Test-only dispatch observer. Production CanonicalHost still rejects model
/// tools until its prepared wrappers are registered in the subsequent adapter.
#[derive(Debug)]
struct Gate {
    host: CanonicalHost,
    starts: Arc<AtomicUsize>,
    reject_terminal: bool,
    strict: bool,
}
struct Permit(Box<dyn HostWorkPermit>);
impl HostWorkPermit for Permit {
    fn response_capture(&self) -> Option<HostResponseCapture> {
        self.0.response_capture()
    }
    fn response_deadline(&self) -> Option<std::time::Instant> {
        self.0.response_deadline()
    }
    fn complete(&mut self) -> Result<(), String> {
        Err("synthetic terminal rejection".into())
    }
}
impl HostWorkAdmission for Gate {
    fn requires_completed_response(&self) -> bool {
        if self.strict {
            self.host.requires_completed_response()
        } else {
            self.host.lifecycle().requires_completed_response()
        }
    }
    fn admit_startup(
        &self,
        workspace: &std::path::Path,
        resumed: Option<codex_protocol::ThreadId>,
    ) -> Result<Box<dyn Send>, String> {
        self.host.admit_startup(workspace, resumed)
    }
    fn admit(
        &self,
        thread: codex_protocol::ThreadId,
        kind: HostWorkKind,
        label: &str,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.host.admit(thread, kind, label)
    }
    fn admit_model(
        &self,
        thread: codex_protocol::ThreadId,
        body: &mut serde_json::Value,
        purpose: HostModelPurpose,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        let permit = self.host.admit_model(thread, body, purpose)?;
        Ok(if self.reject_terminal {
            Box::new(Permit(permit))
        } else {
            permit
        })
    }
    fn admit_tool(
        &self,
        thread: codex_protocol::ThreadId,
        call_id: &str,
        name: &ToolName,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        if !AllowedTools(vec![ToolName::plain("vcp_boundary_fixture")]).contains(name) {
            return Err("unregistered fixture tool".into());
        }
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.host
            .lifecycle()
            .admit(thread, HostWorkKind::Tool, call_id)
    }
}
#[derive(Clone)]
struct Tool(std::path::PathBuf);
impl ToolContributor for Tool {
    fn tools(
        &self,
        _: &ExtensionData,
        _: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        vec![Arc::new(self.clone())]
    }
}
impl<'call> ToolExecutor<ToolCall<'call>> for Tool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("vcp_boundary_fixture")
    }
    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "vcp_boundary_fixture".into(),
            description: "Synthetic response-boundary observer.".into(),
            strict: false,
            parameters: Default::default(),
            output_schema: None,
            defer_loading: None,
        })
    }
    fn handle<'a>(&'a self, _: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            std::fs::write(&self.0, b"one observed effect")
                .map_err(|e| FunctionCallError::Fatal(e.to_string()))?;
            Ok(
                Box::new(JsonToolOutput::new(serde_json::json!({"observed":true})))
                    as Box<dyn ToolOutput>,
            )
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn complete_response_boundary_precedes_tools_and_failed_streams_never_dispatch() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in ["complete", "eof", "rejected", "interrupted", "legacy"] {
            let complete = matches!(mode, "complete" | "legacy");
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let marker = workspace.join("boundary-marker");
            let config = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let binding = task(&host, &config, config.root_task.clone(), None);
            let starts = Arc::new(AtomicUsize::new(0));
            let (release, gate) = tokio::sync::oneshot::channel();
            let prefix = sse(vec![
                ev_response_created("boundary"),
                ev_function_call("boundary-call", "vcp_boundary_fixture", "{}"),
                ev_message_item_added("barrier", ""),
                ev_output_text_delta("call arrived; terminal withheld"),
            ]);
            let terminal = if mode == "eof" {
                String::new()
            } else {
                sse(vec![ev_completed_with_tokens("boundary", 7)])
            };
            let (server, _) = start_streaming_sse_server(vec![
                vec![
                    StreamingSseChunk {
                        gate: None,
                        body: prefix.clone(),
                    },
                    StreamingSseChunk {
                        gate: Some(gate),
                        body: terminal,
                    },
                ],
                vec![StreamingSseChunk {
                    gate: None,
                    body: sse(vec![
                        ev_assistant_message("done", "observed"),
                        ev_completed_with_tokens("followup", 7),
                    ]),
                }],
            ])
            .await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(Gate {
                host: host.clone(),
                starts: starts.clone(),
                reject_terminal: mode == "rejected",
                strict: mode != "legacy",
            }));
            registry.tool_contributor(Arc::new(Tool(marker.clone())));
            let starter = host.clone();
            let cwd = workspace.clone();
            let model = config.price.model.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key(
                    "synthetic-boundary-fixture",
                ))
                .with_allowed_tools(AllowedTools(vec![ToolName::plain("vcp_boundary_fixture")]))
                .with_config(move |c| {
                    c.cwd = cwd.try_into().unwrap();
                    c.model = Some(model.clone());
                    configure_fixture_provider(c);
                    starter
                        .lifecycle()
                        .authorize_startup(c.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_streaming_server(&server)
                .await
                .unwrap();
            let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(id, binding.clone()).unwrap();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "synthetic response-boundary request".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            tokio::time::timeout(
                Duration::from_secs(10),
                wait_for_event(&test.codex, |e| {
                    matches!(e, EventMsg::AgentMessageContentDelta(_))
                }),
            )
            .await
            .unwrap();
            // The core consumed an event after the complete tool item. Give an
            // incorrectly eager Tokio task a chance to reach the independent gate.
            tokio::time::sleep(Duration::from_millis(100)).await;
            if mode == "legacy" {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !marker.exists() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .unwrap();
            }
            assert_eq!(
                starts.load(Ordering::SeqCst),
                usize::from(mode == "legacy"),
                "early dispatch: {mode}"
            );
            assert_eq!(
                marker.exists(),
                mode == "legacy",
                "early native effect: {mode}"
            );
            if mode == "interrupted" {
                host.lifecycle()
                    .hold(id, &host.lifecycle().inspect(id).unwrap().revision)
                    .unwrap()
                    .wait()
                    .await
                    .unwrap();
                let _ = release.send(());
            } else {
                release.send(()).unwrap();
                tokio::time::timeout(
                    Duration::from_secs(10),
                    wait_for_event(&test.codex, |e| {
                        if complete {
                            matches!(e, EventMsg::TurnComplete(_))
                        } else {
                            matches!(e, EventMsg::Error(_))
                        }
                    }),
                )
                .await
                .unwrap();
            }
            assert_eq!(
                starts.load(Ordering::SeqCst),
                usize::from(complete),
                "{mode}"
            );
            assert_eq!(marker.exists(), complete, "{mode}");
            let state = host.snapshot().unwrap();
            let attempts: Vec<Attempt> = state
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .map(|r| r.decode().unwrap())
                .collect();
            if complete {
                assert_eq!(attempts.len(), 2);
                assert!(attempts
                    .iter()
                    .all(|a| a.phase == ReservationState::Settled));
                assert_eq!(server.requests().await.len(), 2);
            } else {
                assert_eq!(attempts.len(), 1);
                assert_eq!(attempts[0].phase, ReservationState::ReconciliationPending);
                assert_eq!(server.requests().await.len(), 1);
                let responses: Vec<ArtifactDescriptor> = state
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Artifact)
                    .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
                    .filter(|a| a.spec.channel == Channel::Response)
                    .collect();
                assert_eq!(responses.len(), 1);
                assert_eq!(responses[0].state, CaptureState::Aborted);
                assert!(host
                    .read_artifact(responses[0].spec.id.clone())
                    .unwrap()
                    .starts_with(prefix.as_bytes()));
            }
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            server.shutdown().await;
        }
    }
}
