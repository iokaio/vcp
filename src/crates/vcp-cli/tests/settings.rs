// SPDX-License-Identifier: Apache-2.0
use std::fs;
use vcp_cli::settings::{local_path, read_bounded};

#[test]
fn existing_configuration_file_stays_a_file_and_workspace_data_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let profile = temp.path().join("profile.json");
    fs::write(&profile, b"{}").unwrap();
    let resolved = local_path(&profile, &workspace).unwrap();
    assert_eq!(read_bounded(&resolved, 2).unwrap(), b"{}");
    assert!(read_bounded(&resolved, 1).is_err());
    assert!(local_path(&workspace.join("data"), &workspace).is_err());
    let traversal = std::path::PathBuf::from(format!(
        "{}{}..{}elsewhere",
        workspace.display(),
        std::path::MAIN_SEPARATOR,
        std::path::MAIN_SEPARATOR
    ));
    assert!(local_path(&traversal, &workspace).is_err());
    let other = temp.path().join("repository");
    fs::create_dir(&other).unwrap();
    fs::write(other.join(".git"), "gitdir: other").unwrap();
    assert!(local_path(&other.join("data"), &workspace).is_err());
    #[cfg(windows)]
    assert!(local_path(
        &std::path::PathBuf::from(workspace.to_string_lossy().to_uppercase()).join("DATA"),
        &workspace
    )
    .is_err());
}
