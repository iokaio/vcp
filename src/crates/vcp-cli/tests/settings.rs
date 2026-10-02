// SPDX-License-Identifier: Apache-2.0
use std::fs;
use vcp_cli::settings::{
    display_path, local_path, placement_conflict_with, read_bounded, PlacementConflict,
};

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

#[test]
fn placement_conflicts_name_the_containing_root() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let synced = temp.path().join("synced");
    let repository = temp.path().join("repository");
    for directory in [&workspace, &synced, &repository] {
        fs::create_dir(directory).unwrap();
    }
    fs::write(repository.join(".git"), "gitdir: other").unwrap();
    let [workspace, synced, repository] =
        [workspace, synced, repository].map(|path| path.canonicalize().unwrap());
    let roots = [("OneDriveCommercial".to_owned(), synced.clone())];
    assert_eq!(
        placement_conflict_with(&workspace.join("data"), &workspace, &roots),
        Some(PlacementConflict::Workspace(workspace.clone()))
    );
    assert_eq!(
        placement_conflict_with(&synced.join("data"), &workspace, &roots),
        Some(PlacementConflict::SyncRoot {
            variable: "OneDriveCommercial".into(),
            root: synced.clone()
        })
    );
    assert_eq!(
        placement_conflict_with(&repository.join("nested"), &workspace, &roots),
        Some(PlacementConflict::Repository(repository.clone()))
    );
    assert_eq!(
        placement_conflict_with(&temp.path().canonicalize().unwrap(), &workspace, &roots),
        None
    );
    let error = local_path(&workspace.join("data"), &workspace).unwrap_err();
    assert!(
        error.contains(&format!(
            "inside the selected workspace {}",
            display_path(&workspace)
        )),
        "{error}"
    );
    let error = local_path(&repository.join("data"), &workspace).unwrap_err();
    assert!(
        error.contains(&format!(
            "the Git repository at {}",
            display_path(&repository)
        )),
        "{error}"
    );
}

#[test]
fn displayed_paths_omit_the_verbatim_prefix_only_for_drives() {
    use std::path::Path;
    assert_eq!(display_path(Path::new(r"\\?\C:\Users\me")), r"C:\Users\me");
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share")),
        r"\\?\UNC\server\share"
    );
    assert_eq!(display_path(Path::new(r"D:\code")), r"D:\code");
}
