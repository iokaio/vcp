// SPDX-License-Identifier: Apache-2.0
//! Local physical binding evidence, independent of mutable repository contents.
use serde::{Deserialize, Serialize};
use std::{io::ErrorKind, path::PathBuf};
use vcp_repository::Root;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceIdentity {
    pub version: u32,
    pub root_path: PathBuf,
    pub directory_identity: String,
    pub git_directory_identity: Option<String>,
}

fn reconcile(reason: &str) -> String {
    format!("workspace binding requires rebind/reconcile: {reason}; retained history is preserved")
}

/// Capture identity only during authorized registration/reconciliation. In
/// particular, an absent legacy descriptor is not evidence that a root matches.
/// Windows native handles reject reparse points and pin each traversed ancestor.
/// No Git configuration, hooks, filters, or executable are invoked.
pub fn capture(root: &Root) -> Result<WorkspaceIdentity, String> {
    let directory = root
        .hold(None, true)
        .map_err(|_| reconcile("root is missing, replaced, or inaccessible"))?;
    let git = match root.hold(Some(std::path::Path::new(".git")), true) {
        Ok(git) => Some(git),
        Err(vcp_repository::Error::Io(error)) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => return Err(reconcile(
            "Git metadata is inaccessible or uses an unsupported linked-worktree/reparse layout",
        )),
    };
    if git.is_some() {
        // External metadata needs a separately registered scope. Match the
        // repository reader's trust boundary instead of following pointer text.
        for path in [".git/commondir", ".git/objects/info/alternates"] {
            match root.hold(Some(std::path::Path::new(path)), false) {
                Err(vcp_repository::Error::Io(error)) if error.kind() == ErrorKind::NotFound => (),
                _ => {
                    return Err(reconcile(
                        "external Git metadata requires an explicitly registered scope",
                    ))
                }
            }
        }
    }
    Ok(WorkspaceIdentity {
        version: 1,
        root_path: root.path().to_owned(),
        directory_identity: directory.native_identity.clone(),
        git_directory_identity: git.map(|held| held.native_identity.clone()),
    })
}

/// Verify immediately before continuation preparation. This is discovery
/// evidence, not a substitute for the mutation broker's dispatch-time checks.
pub fn verify(root: &Root, expected: &WorkspaceIdentity) -> Result<(), String> {
    if expected.version != 1 {
        return Err(reconcile("unsupported binding identity version"));
    }
    let current = capture(root)?;
    if current != *expected {
        return Err(reconcile("root or Git worktree identity changed"));
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{fs, path::Path};
    use vcp_domain::{Revision, RootId, WorkspaceId};
    use vcp_repository::RootIdentity;

    fn root(path: &Path) -> Root {
        Root::open(
            RootIdentity {
                workspace: WorkspaceId::new(),
                root: RootId::new(),
                repository: "fixture".into(),
                worktree: "fixture".into(),
                binding: Revision::ZERO,
            },
            path,
        )
        .unwrap()
    }

    #[test]
    fn plain_directory_identity_survives_content_changes_but_not_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("workspace");
        fs::create_dir(&path).unwrap();
        let original = root(&path);
        let identity = capture(&original).unwrap();
        fs::write(path.join("source.rs"), "changed").unwrap();
        verify(&original, &identity).unwrap();
        fs::rename(&path, temp.path().join("preserved")).unwrap();
        assert!(verify(&original, &identity)
            .unwrap_err()
            .contains("rebind/reconcile"));
        fs::create_dir(&path).unwrap();
        assert!(verify(&root(&path), &identity).is_err());
        assert!(verify(&root(&temp.path().join("preserved")), &identity).is_err());
        assert_eq!(
            fs::read_to_string(temp.path().join("preserved/source.rs")).unwrap(),
            "changed"
        );
    }

    #[test]
    fn git_metadata_replacement_and_removal_require_reconciliation() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        let workspace = root(temp.path());
        let identity = capture(&workspace).unwrap();
        fs::write(temp.path().join(".git/HEAD"), "ref: refs/heads/main").unwrap();
        fs::write(
            temp.path().join(".git/config"),
            "[core]\nfsmonitor = must-not-run",
        )
        .unwrap();
        verify(&workspace, &identity).unwrap();
        fs::rename(temp.path().join(".git"), temp.path().join("old-git")).unwrap();
        assert!(verify(&workspace, &identity).is_err());
        fs::create_dir(temp.path().join(".git")).unwrap();
        assert!(verify(&workspace, &identity).is_err());
    }

    #[test]
    fn git_initialization_external_metadata_and_unknown_versions_are_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = root(temp.path());
        let mut identity = capture(&workspace).unwrap();
        identity.version = 2;
        assert!(verify(&workspace, &identity).is_err());
        identity.version = 1;
        fs::write(temp.path().join(".git"), "gitdir: ../external").unwrap();
        assert!(capture(&workspace).unwrap_err().contains("linked-worktree"));
        fs::remove_file(temp.path().join(".git")).unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        assert!(verify(&workspace, &identity).is_err());
        fs::write(temp.path().join(".git/commondir"), "../external").unwrap();
        assert!(capture(&workspace)
            .unwrap_err()
            .contains("explicitly registered scope"));
    }
}
