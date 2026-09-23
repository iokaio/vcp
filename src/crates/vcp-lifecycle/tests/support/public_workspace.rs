// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::methods::{self, Call, ResultValue};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn access(config: &Config) -> Access {
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: false,
        bootstrap: false,
    }
}
fn request(config: &Config) -> methods::WorkspaceOpen {
    methods::WorkspaceOpen {
        command_id: id("existing-open-is-not-a-command"),
        host: id(config.binding.host.as_str()),
        root: config.binding.root.clone(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_workspace_open_observes_only_selected_binding_without_mutation_or_lease() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let config = config(
            &temp.path().join("canonical"),
            &workspace.canonicalize().unwrap(),
            backend,
        );
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let access = access(&config);
        let mut observer = host.public_connection(access.clone()).unwrap();
        assert!(observer.supported_methods().contains(&"workspace/open"));
        assert!(observer.controller_token().is_err());
        let before = host.snapshot().unwrap();
        let request = request(&config);
        let call = Call::WorkspaceOpen(request.clone());
        // Registry conservatively reserves mutation identity for a possible
        // future creation variant. This existing-binding facade is read-only.
        assert!(call.is_mutation());
        let ResultValue::Workspace(view) = observer.call(call.clone(), &access).await.unwrap()
        else {
            panic!("workspace projection required");
        };
        assert_eq!(view.workspace.as_str(), config.workspace.as_str());
        assert_eq!(view.host.as_str(), config.binding.host.as_str());
        assert_eq!(view.root, config.binding.root);
        assert_eq!(view.trust, methods::Trust::Untrusted);
        assert_eq!(view.revision.as_str(), "0");
        assert_eq!(view.authority_revision.as_str(), "0");
        assert_eq!(
            observer.call(call, &access).await.unwrap(),
            ResultValue::Workspace(view)
        );
        for root in [
            "relative".to_owned(),
            format!("{}\\..\\elsewhere", config.binding.root),
            r"\\unrequested-host\unrequested-share".to_owned(),
            temp.path()
                .join("must-not-create")
                .to_string_lossy()
                .into_owned(),
        ] {
            let mut other = request.clone();
            other.root = root;
            assert!(observer.workspace_open(&other, &access).is_err());
        }
        let mut other = request.clone();
        other.host = id("other-host");
        assert!(observer.workspace_open(&other, &access).is_err());
        assert!(!temp.path().join("must-not-create").exists());
        assert_eq!(host.snapshot().unwrap(), before);
        assert!(observer.controller_token().is_err());
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_workspace_open_rechecks_access_authority_binding_and_connection_loss() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let config = config(
            &temp.path().join("canonical"),
            &workspace.canonicalize().unwrap(),
            backend,
        );
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let mut current = access(&config);
        let observer = host.public_connection(current.clone()).unwrap();
        let request = request(&config);
        let before = host.snapshot().unwrap();
        for case in [
            "actor",
            "workspace",
            "session",
            "read",
            "write",
            "bootstrap",
            "authority",
        ] {
            let mut denied = current.clone();
            match case {
                "actor" => denied.actor = ActorId::new(),
                "workspace" => denied.workspace = WorkspaceId::new(),
                "session" => denied.session = SessionId::new(),
                "read" => denied.read = false,
                "write" => denied.write = true,
                "bootstrap" => denied.bootstrap = true,
                "authority" => denied.authority = AuthorityRevision::new(1),
                _ => unreachable!(),
            }
            // Current authorization precedes even malformed request validation.
            let mut malformed = request.clone();
            malformed.root.clear();
            let error = observer.workspace_open(&malformed, &denied).unwrap_err();
            assert_ne!(error.code, -32602, "{case}");
        }
        assert_eq!(host.snapshot().unwrap(), before);
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        assert!(observer.workspace_open(&request, &current).is_err());
        current.authority = AuthorityRevision::new(1);
        let changed = host.snapshot().unwrap();
        let view = observer.workspace_open(&request, &current).unwrap();
        assert_eq!(view.trust, methods::Trust::Trusted);
        assert_eq!(view.revision.as_str(), "1");
        assert_eq!(view.authority_revision.as_str(), "1");
        assert_eq!(host.snapshot().unwrap(), changed);
        observer.loss_signal().invalidate();
        assert!(observer.workspace_open(&request, &current).is_err());
        assert_eq!(host.snapshot().unwrap(), changed);
        observer.disconnect().unwrap().wait().await.unwrap();

        let observer = host.public_connection(current.clone()).unwrap();
        let mut rebound = config.binding.clone();
        rebound.root = temp
            .path()
            .join("replacement")
            .to_string_lossy()
            .into_owned();
        host.command(
            Command::Rebind {
                binding: rebound.clone(),
            },
            None,
            Revision::new(1),
        )
        .unwrap();
        current.authority = AuthorityRevision::new(2);
        let changed = host.snapshot().unwrap();
        assert!(observer.workspace_open(&request, &current).is_err());
        let mut replacement = request;
        replacement.root = rebound.root;
        assert!(observer.workspace_open(&replacement, &current).is_err());
        assert_eq!(host.snapshot().unwrap(), changed);
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}
