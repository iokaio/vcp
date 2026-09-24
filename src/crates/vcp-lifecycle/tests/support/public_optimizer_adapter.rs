// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{rpc::RpcHost, Access};
use vcp_lifecycle::foundation::routing::Request as Cli;
use vcp_protocol::{
    methods::{self, Call, ResultValue},
    routing_optimizer as wire,
};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn lease_revision(host: &CanonicalHost) -> Revision {
    host.snapshot()
        .unwrap()
        .records
        .values()
        .find(|r| r.value["document_type"] == "vcp_controller_lease_v1")
        .unwrap()
        .decode::<vcp_domain::controller::Lease>()
        .unwrap()
        .revision
}
fn scope(config: &Config) -> methods::Scope {
    methods::Scope {
        workspace: id(config.workspace.as_str()),
        session: id(config.session.as_str()),
    }
}
fn workspace(host: &CanonicalHost, config: &Config) -> Workspace {
    host.snapshot()
        .unwrap()
        .record(
            Collection::Workspace,
            config.workspace.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}
fn access(host: &CanonicalHost, config: &Config, write: bool) -> Access {
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: workspace(host, config).authority,
        read: true,
        write,
        bootstrap: false,
    }
}
fn mutation(command: &str, workspace: &Workspace) -> methods::Mutation {
    methods::Mutation {
        command_id: id(command),
        expected_revision: workspace.revision.get().into(),
        steering_revision: 0.into(),
    }
}
fn capture(
    config: &Config,
    workspace: &Workspace,
    command: &str,
    coverage: wire::Coverage,
) -> Call {
    Call::RoutingReportCapture(wire::ReportCapture {
        scope: scope(config),
        mutation: mutation(command, workspace),
        expected_binding_revision: workspace.binding.revision.get().into(),
        window: wire::Window {
            from: None,
            until: u64::MAX.into(),
        },
        coverage,
    })
}
fn read(config: &Config, report: &str) -> Call {
    Call::RoutingReportRead(wire::ReportRead {
        scope: scope(config),
        report: id(report),
        section: wire::ReportSection::Summary,
        limit: 1,
        cursor: None,
    })
}
fn proposal(config: &Config, report: &str, revision: u64) -> Call {
    Call::RoutingPreview(wire::PreviewRequest {
        scope: scope(config),
        expected_policy_revision: revision.into(),
        proposal: wire::Proposal::Apply {
            report: id(report),
            edits: vec![wire::Edit::MinimumSamples(21)],
        },
    })
}
fn apply(
    config: &Config,
    workspace: &Workspace,
    command: &str,
    preview: &wire::PreviewView,
) -> Call {
    Call::RoutingApply(wire::Apply {
        scope: scope(config),
        mutation: mutation(command, workspace),
        expected_binding_revision: workspace.binding.revision.get().into(),
        preview_id: preview.preview_id.clone(),
        preview_sha256: preview.preview_sha256.clone(),
    })
}
fn view(result: ResultValue) -> wire::PreviewView {
    let ResultValue::RoutingPreview(v) = result else {
        panic!("preview required")
    };
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn optimizer_public_review_commands_preserve_control_replay_and_cli_staleness() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let config = config(
            &temp.path().join("canonical"),
            &root.canonicalize().unwrap(),
            backend,
        );
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        host.configure_routing(super::routing::routing_configuration(
            vcp_models::routing::Profile::Low,
            false,
        ))
        .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "optimizer metadata fixture".into(),
                verification: None,
            },
            Some(binding.scope.task),
            Revision::new(1),
        )
        .unwrap();
        let auth = access(&host, &config, true);
        let observer_auth = access(&host, &config, false);
        let mut controller = host.public_connection(auth.clone()).unwrap();
        controller.acquire(CommandId::new(), None).unwrap();
        let mut observer = host.public_connection(observer_auth.clone()).unwrap();
        let original = workspace(&host, &config);
        let mut wrong_binding = capture(
            &config,
            &original,
            "optimizer-wrong-binding",
            wire::Coverage::Session,
        );
        if let Call::RoutingReportCapture(value) = &mut wrong_binding {
            value.expected_binding_revision = (original.binding.revision.get() + 1).into();
        }
        assert!(controller.call(wrong_binding, &auth).await.is_err());
        let capture_call = capture(
            &config,
            &original,
            "optimizer-session",
            wire::Coverage::Session,
        );
        assert!(observer
            .call(capture_call.clone(), &observer_auth)
            .await
            .is_err());
        let receipt = controller.call(capture_call.clone(), &auth).await.unwrap();
        assert_eq!(
            controller.call(capture_call.clone(), &auth).await.unwrap(),
            receipt
        );
        let mut altered = capture_call.clone();
        if let Call::RoutingReportCapture(v) = &mut altered {
            v.coverage = wire::Coverage::Workspace;
        }
        assert!(controller.call(altered, &auth).await.is_err());
        let ResultValue::RoutingReport(page) = observer
            .call(read(&config, "optimizer-session"), &observer_auth)
            .await
            .unwrap()
        else {
            panic!("scoped report")
        };
        assert_eq!(page.report.as_str(), "optimizer-session");
        assert_eq!(page.coverage, wire::Coverage::Session);
        assert_eq!(page.rows.len(), 1);
        controller
            .call(
                capture(
                    &config,
                    &original,
                    "optimizer-workspace",
                    wire::Coverage::Workspace,
                ),
                &auth,
            )
            .await
            .unwrap();
        assert!(observer
            .call(read(&config, "optimizer-workspace"), &observer_auth)
            .await
            .is_err());
        assert!(observer
            .call(proposal(&config, "optimizer-session", 0), &observer_auth)
            .await
            .is_err());
        let preview = view(
            controller
                .call(proposal(&config, "optimizer-session", 0), &auth)
                .await
                .unwrap(),
        );
        assert_eq!(preview.assessment, wire::Assessment::NotDispatchAuthority);
        assert_eq!(preview.prior.summary.minimum_samples, 20);
        assert_eq!(preview.persisted.summary.minimum_samples, 21);
        assert_eq!(preview.persisted.allowed_models.len(), 2);
        assert!(preview.changed.contains(&wire::Field::MinimumSamples));
        let apply_call = apply(&config, &original, "optimizer-apply", &preview);
        assert!(observer
            .call(apply_call.clone(), &observer_auth)
            .await
            .is_err());
        let applied = controller.call(apply_call.clone(), &auth).await.unwrap();
        assert_eq!(
            controller.call(apply_call.clone(), &auth).await.unwrap(),
            applied
        );
        let stale = view(
            controller
                .call(proposal(&config, "optimizer-session", 1), &auth)
                .await
                .unwrap(),
        );
        host.routing_control(Cli::Rollback {
            command: CommandId::new(),
            expected: Revision::new(1),
            target: Revision::ZERO,
        })
        .unwrap();
        assert!(controller
            .call(apply(&config, &original, "optimizer-stale", &stale), &auth)
            .await
            .is_err());
        let rollback = view(
            controller
                .call(
                    Call::RoutingPreview(wire::PreviewRequest {
                        scope: scope(&config),
                        expected_policy_revision: 2.into(),
                        proposal: wire::Proposal::Rollback {
                            target_revision: 1.into(),
                        },
                    }),
                    &auth,
                )
                .await
                .unwrap(),
        );
        assert_eq!(rollback.operation, wire::Operation::Rollback);
        assert_eq!(
            rollback.target_policy_revision.as_ref().unwrap().as_str(),
            "1"
        );
        let rollback_call = Call::RoutingRollback(wire::Rollback {
            scope: scope(&config),
            mutation: mutation("optimizer-rollback", &original),
            expected_binding_revision: original.binding.revision.get().into(),
            preview_id: rollback.preview_id,
            preview_sha256: rollback.preview_sha256,
        });
        let rolled = controller.call(rollback_call.clone(), &auth).await.unwrap();
        assert_eq!(
            controller.call(rollback_call.clone(), &auth).await.unwrap(),
            rolled
        );
        let current = view(
            controller
                .call(proposal(&config, "optimizer-session", 3), &auth)
                .await
                .unwrap(),
        );
        controller.disconnect().unwrap().wait().await.unwrap();
        let mut replacement = host.public_connection(auth.clone()).unwrap();
        replacement
            .acquire(CommandId::new(), Some(lease_revision(&host)))
            .unwrap();
        assert!(replacement
            .call(
                apply(&config, &original, "optimizer-lost-preview", &current),
                &auth
            )
            .await
            .is_err());
        assert_eq!(replacement.call(apply_call, &auth).await.unwrap(), applied);
        assert_eq!(
            replacement.call(rollback_call, &auth).await.unwrap(),
            rolled
        );
        let ResultValue::Acceptance(command) = replacement
            .call(
                Call::CommandRead(methods::CommandRead {
                    scope: scope(&config),
                    command_id: id("optimizer-apply"),
                }),
                &auth,
            )
            .await
            .unwrap()
        else {
            panic!("command receipt")
        };
        assert!(serde_json::to_string(&command)
            .unwrap()
            .contains("accepted"));
        observer.disconnect().unwrap().wait().await.unwrap();
        assert!(replacement.controller_token().is_ok());
        let trust_preview = view(
            replacement
                .call(proposal(&config, "optimizer-session", 3), &auth)
                .await
                .unwrap(),
        );
        replacement
            .call(
                Call::WorkspaceSetTrust(methods::WorkspaceSetTrust {
                    scope: scope(&config),
                    mutation: mutation("optimizer-untrust", &original),
                    expected_binding_revision: original.binding.revision.get().into(),
                    trusted: false,
                }),
                &auth,
            )
            .await
            .unwrap();
        let fresh_auth = access(&host, &config, true);
        assert!(replacement
            .call(
                apply(
                    &config,
                    &workspace(&host, &config),
                    "optimizer-untrusted",
                    &trust_preview
                ),
                &fresh_auth
            )
            .await
            .is_err());
        assert!(replacement
            .call(proposal(&config, "optimizer-session", 3), &fresh_auth)
            .await
            .is_err());
        replacement.disconnect().unwrap().wait().await.unwrap();
        let mut untrusted = host.public_connection(fresh_auth.clone()).unwrap();
        untrusted
            .acquire(CommandId::new(), Some(lease_revision(&host)))
            .unwrap();
        let before = host.snapshot().unwrap();
        let error = untrusted
            .call(proposal(&config, "optimizer-session", 3), &fresh_auth)
            .await
            .unwrap_err();
        assert!(serde_json::to_string(&error)
            .unwrap()
            .contains("POLICY_DENIED"));
        let error = untrusted
            .call(
                apply(
                    &config,
                    &workspace(&host, &config),
                    "optimizer-fresh-untrusted",
                    &trust_preview,
                ),
                &fresh_auth,
            )
            .await
            .unwrap_err();
        assert!(serde_json::to_string(&error)
            .unwrap()
            .contains("POLICY_DENIED"));
        assert_eq!(host.snapshot().unwrap(), before);
        assert!(!host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        untrusted.disconnect().unwrap().wait().await.unwrap();
        drop(owner);
        drop(host);
    }
}
