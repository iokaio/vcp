// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use vcp_domain::{policy::*, workspace::Scope, *};
use vcp_repository::{Root, RootIdentity};
use vcp_tools::{process::*, Identity};
#[test]
fn profiles_reject_model_authority_environment_credentials_and_implicit_shells() {
    assert!(Request::from_arguments(r#"{"profile":"p","arguments":[],"directory":"","timeout_ms":1000,"output_bytes":100,"environment":{}}"#).is_err());
    for field in ["max_timeout_ms", "output_encoding"] {
        let mut request = serde_json::json!({"profile":"p","arguments":[],"directory":"","timeout_ms":1000,"output_bytes":100});
        request[field] = serde_json::json!(180000);
        assert!(Request::from_arguments(&request.to_string()).is_err());
    }

    assert!(Profile::new(
        "p".into(),
        "cmd.exe".into(),
        Mode::Direct,
        BTreeMap::new(),
        BTreeSet::new(),
        true
    )
    .is_err());
    for environment in [
        BTreeMap::from([("OPENROUTER_API_KEY".into(), "synthetic-denied".into())]),
        BTreeMap::from([(
            "CARGO_REGISTRIES_CRATES_IO_TOKEN".into(),
            "synthetic-denied".into(),
        )]),
        BTreeMap::from([("RUSTC_WRAPPER".into(), "synthetic-override.exe".into())]),
        BTreeMap::from([("PATH".into(), "one".into()), ("Path".into(), "two".into())]),
    ] {
        assert!(Profile::new(
            "p".into(),
            r"C:\synthetic\program.exe".into(),
            Mode::Direct,
            environment,
            BTreeSet::new(),
            true
        )
        .is_err());
    }
}
#[test]
fn prepared_process_pins_executable_and_rejects_changed_script_or_directory() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let tools = temp.path().join("host-tools");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&tools).unwrap();
    let executable = tools.join("fixture.exe");
    fs::write(&executable, b"synthetic identity, never executed").unwrap();
    fs::write(workspace.join("script.ps1"), b"before").unwrap();
    let scope = Scope {
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: TaskId::new(),
    };
    let root = Root::open(
        RootIdentity {
            workspace: scope.workspace.clone(),
            root: RootId::new(),
            repository: "fixture".into(),
            worktree: "fixture".into(),
            binding: Revision::ZERO,
        },
        &workspace,
    )
    .unwrap();
    let identity = Identity {
        scope,
        actor: ActorId::new(),
        host: HostId::new(),
        binding: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        steering: SteeringRevision::ZERO,
        policy: PolicyRevision::ZERO,
    };
    let profile = Profile::new(
        "fixture".into(),
        executable.clone(),
        Mode::Direct,
        BTreeMap::new(),
        BTreeSet::new(),
        true,
    )
    .unwrap();
    let profile = profile.with_inputs(vec!["script.ps1".into()]).unwrap();
    let duration_probe = |profile: Profile, duration| {
        prepare(
            Root::open(root.identity.clone(), &workspace).unwrap(),
            Identity {
                scope: identity.scope.clone(),
                actor: identity.actor.clone(),
                host: identity.host.clone(),
                binding: identity.binding,
                authority: identity.authority,
                steering: identity.steering,
                policy: identity.policy,
            },
            profile,
            Request {
                profile: "fixture".into(),
                arguments: vec![],
                directory: String::new(),
                timeout_ms: duration,
                output_bytes: 1024,
                input: None,
            },
        )
    };
    // Reproduce the former direct-profile ceiling without waiting two minutes.
    assert!(duration_probe(profile.clone(), 120_001).is_err());
    assert!(duration_probe(profile.clone(), 120_000).is_ok());
    assert!(profile.clone().with_max_timeout_ms(0).is_err());
    assert!(profile
        .clone()
        .with_max_timeout_ms(MAX_TIMEOUT_MS + 1)
        .is_err());
    let extended = profile.clone().with_max_timeout_ms(180_000).unwrap();
    let prepared = duration_probe(extended.clone(), 120_001).unwrap();
    assert!(duration_probe(extended.clone(), 180_001).is_err());
    assert_ne!(profile.digest().unwrap(), extended.digest().unwrap());
    assert_ne!(
        prepared.authority().digest(),
        duration_probe(extended, 120_002)
            .unwrap()
            .authority()
            .digest()
    );
    assert_ne!(
        profile.digest().unwrap(),
        profile
            .clone()
            .with_output_encoding(Some(output::Encoding::Utf16Le))
            .unwrap()
            .digest()
            .unwrap()
    );

    let mut collision_identity = root.identity.clone();
    collision_identity.root = profile.executable_root_id().unwrap();
    let collision = prepare(
        Root::open(collision_identity, &workspace).unwrap(),
        Identity {
            scope: identity.scope.clone(),
            actor: identity.actor.clone(),
            host: identity.host.clone(),
            binding: identity.binding,
            authority: identity.authority,
            steering: identity.steering,
            policy: identity.policy,
        },
        profile.clone(),
        Request {
            profile: "fixture".into(),
            arguments: vec![],
            directory: String::new(),
            timeout_ms: 1000,
            output_bytes: 1024,
            input: None,
        },
    );
    assert!(matches!(
        collision,
        Err(vcp_tools::Error::Invalid(
            "executable and workspace root identities collide"
        ))
    ));
    let terminal_probe = |terminal: bool, input: &str| {
        let profile = if terminal {
            profile.clone().with_terminal(24, 80).unwrap()
        } else {
            profile.clone()
        };
        prepare(
            Root::open(root.identity.clone(), &workspace).unwrap(),
            Identity {
                scope: identity.scope.clone(),
                actor: identity.actor.clone(),
                host: identity.host.clone(),
                binding: identity.binding,
                authority: identity.authority,
                steering: identity.steering,
                policy: identity.policy,
            },
            profile,
            Request {
                profile: "fixture".into(),
                arguments: vec![],
                directory: String::new(),
                timeout_ms: 1000,
                output_bytes: 1024,
                input: Some(input.into()),
            },
        )
    };
    assert!(terminal_probe(false, "unbound pipe input").is_err());
    assert!(terminal_probe(true, &"x".repeat(32769)).is_err());
    let first = terminal_probe(true, "one\n").unwrap();
    let changed = terminal_probe(true, "two\n").unwrap();
    assert_ne!(first.authority().digest(), changed.authority().digest());
    assert!(first
        .authority()
        .operation()
        .required_isolation
        .contains(&Isolation::Pty));
    assert!(profile.clone().with_terminal(0, 80).is_err());
    assert_eq!(profile.process_count(), 32);
    assert!(profile.clone().with_process_count(0).is_err());
    assert!(profile.clone().with_process_count(129).is_err());
    assert_ne!(
        profile.digest().unwrap(),
        profile
            .clone()
            .with_process_count(2)
            .unwrap()
            .digest()
            .unwrap()
    );
    assert!(first
        .authority()
        .operation()
        .required_isolation
        .contains(&Isolation::ProcessCount));
    let prepared = prepare(
        root,
        identity,
        profile,
        Request {
            profile: "fixture".into(),
            arguments: vec![],
            directory: String::new(),
            timeout_ms: 1000,
            output_bytes: 1024,
            input: None,
        },
    )
    .unwrap();
    assert!(prepared
        .authority()
        .operation()
        .effects
        .contains(&EffectClass::Opaque));
    let pins = prepared.pin().unwrap();
    assert!(fs::write(workspace.join("script.ps1"), b"substitution while running").is_err());
    assert!(fs::write(&executable, b"changed").is_err());
    assert!(fs::rename(&workspace, temp.path().join("moved")).is_err());
    drop(pins);
    fs::write(workspace.join("script.ps1"), b"human").unwrap();
    assert!(prepared.pin().is_err());
    fs::write(workspace.join("script.ps1"), b"before").unwrap();
    prepared.pin().unwrap();
    fs::write(&executable, b"changed").unwrap();
    assert!(prepared.pin().is_err());
}
