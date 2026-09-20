// SPDX-License-Identifier: Apache-2.0
use vcp_cli::backup::{self, Keys};

#[test]
fn structured_commands_keep_secret_material_out_of_literal_arguments() {
    use clap::Parser;
    use vcp_cli::args::Cli;
    assert!(Cli::try_parse_from(["vcp", "workspace", "rebind", "workspace-fixture"]).is_ok());
    assert!(Cli::try_parse_from([
        "vcp",
        "backup",
        "keys",
        "--workspace-id",
        "workspace-fixture",
        "verify",
        "--key",
        "independent.recovery"
    ])
    .is_ok());
    assert!(Cli::try_parse_from([
        "vcp",
        "backup",
        "keys",
        "verify",
        "--secret-key",
        "literal-secret"
    ])
    .is_err());
    assert!(Cli::try_parse_from([
        "vcp",
        "storage",
        "migrate",
        "--backend",
        "sqlite",
        "--preview"
    ])
    .is_ok());
}

#[test]
fn independent_key_copies_rotate_without_serializing_secrets_and_import_on_fresh_host() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let workspace = base.join("workspace");
    let data = base.join("data");
    let recovery = base.join("independent-recovery");
    let sync = base.join("declared-sync");
    for directory in [&workspace, &data, &recovery, &sync] {
        std::fs::create_dir(directory).unwrap();
    }
    let created = backup::keys(
        &Keys::Create {
            recovery_dir: recovery.clone(),
            sync_root: vec![sync.clone()],
        },
        &data,
        &workspace,
        None,
        Some("workspace-fixture"),
    )
    .unwrap();
    assert_eq!(created["independent_recovery_verified"], true);
    let copy = std::path::PathBuf::from(created["recovery_copy"].as_str().unwrap());
    let serialized = serde_json::to_string(&created).unwrap();
    assert!(!serialized.contains("AGE-SECRET-KEY"));
    assert!(!serialized.contains("VCP writer Ed25519"));
    backup::keys(
        &Keys::Verify {
            key: copy.clone(),
            sync_root: vec![],
        },
        &data,
        &workspace,
        None,
        Some("workspace-fixture"),
    )
    .unwrap();
    let second_data = base.join("fresh-host-data");
    std::fs::create_dir(&second_data).unwrap();
    let checkpoint = base.join("independent-checkpoint.json");
    std::fs::write(
        &checkpoint,
        serde_json::to_vec(&created["configuration"]["checkpoint"]).unwrap(),
    )
    .unwrap();
    let imported = backup::keys(
        &Keys::Import {
            key: copy.clone(),
            lineage: created["configuration"]["lineage"].as_str().unwrap().into(),
            checkpoint,
            sync_root: vec![],
        },
        &second_data,
        &workspace,
        None,
        Some("workspace-fixture"),
    )
    .unwrap();
    assert_eq!(
        imported["configuration"]["selected"],
        created["configuration"]["selected"]
    );
    assert_eq!(imported["configuration"]["global_newest_known"], false);
    let rotated = backup::keys(
        &Keys::Rotate {
            recovery_dir: recovery.clone(),
            expected_revision: 0,
            sync_root: vec![],
        },
        &data,
        &workspace,
        None,
        Some("workspace-fixture"),
    )
    .unwrap();
    assert_ne!(
        rotated["configuration"]["selected"],
        created["configuration"]["selected"]
    );
    assert!(copy.is_file());
    assert!(backup::keys(
        &Keys::Verify {
            key: copy,
            sync_root: vec![]
        },
        &data,
        &workspace,
        None,
        Some("workspace-fixture")
    )
    .is_err());
    assert!(backup::keys(
        &Keys::Create {
            recovery_dir: sync.clone(),
            sync_root: vec![sync]
        },
        &data,
        &workspace,
        None,
        Some("other-workspace")
    )
    .is_err());
}

#[test]
fn selected_descriptor_is_leased_until_the_old_owner_finishes() {
    use vcp_cli::selection::Lease;
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().canonicalize().unwrap();
    let entry = data.join("workspaces/entry");
    std::fs::create_dir_all(&entry).unwrap();
    let owner = Lease::shared(&data, &entry).unwrap();
    let inspector = Lease::shared(&data, &entry).unwrap();
    assert!(Lease::exclusive(&data, &entry).is_err());
    drop(owner);
    assert!(Lease::exclusive(&data, &entry).is_err());
    drop(inspector);
    let mut activation = Lease::exclusive(&data, &entry).unwrap();
    assert!(Lease::shared(&data, &entry).is_err());
    let target = activation.target(&vcp_domain::CommandId::new()).unwrap();
    assert_eq!(
        target.parent(),
        Some(entry.join("canonical-roots").as_path())
    );
    assert!(activation
        .target(&vcp_domain::CommandId::parse("arbitrary-name").unwrap())
        .is_err());
    drop(activation);
    Lease::shared(&data, &entry).unwrap();
}
