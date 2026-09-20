// SPDX-License-Identifier: Apache-2.0
use vcp_extensions::catalog;

#[test]
fn embedded_inventory_is_closed_versioned_metadata_for_all_families() {
    let manifest = catalog::embedded().unwrap();
    assert_eq!(manifest.skills.len(), 21);
    assert_eq!(manifest.version, "1.0.0");
    let mut invalid = manifest.clone();
    invalid.skills[1] = invalid.skills[0].clone();
    assert!(invalid.validate().is_err());
    let mut invalid = manifest.clone();
    invalid.skills[0].body.path = "../escape".into();
    assert!(invalid.validate().is_err());
    let mut invalid = serde_json::to_value(&manifest).unwrap();
    invalid["authority"] = serde_json::json!(true);
    assert!(serde_json::from_value::<catalog::Manifest>(invalid).is_err());
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{collections::BTreeSet, fs, path::Path};
    use vcp_domain::{Revision, RootId, WorkspaceId};
    use vcp_extensions::{activation, discovery, skill_manifest::*};
    use vcp_repository::{Root, RootIdentity};

    fn source_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/builtin")
    }
    fn staged(path: &Path, bodies: bool) -> (Root, SourceRegistry) {
        let source = source_path();
        let manifest = catalog::embedded().unwrap();
        for file in ["catalog.json", &manifest.coverage.path] {
            fs::copy(source.join(file), path.join(file)).unwrap();
        }
        for skill in &manifest.skills {
            fs::create_dir(path.join(&skill.id)).unwrap();
            fs::copy(source.join(&skill.descriptor), path.join(&skill.descriptor)).unwrap();
            if bodies {
                for content in std::iter::once(&skill.body).chain(&skill.resources) {
                    let relative = Path::new(&skill.id).join(&content.path);
                    fs::create_dir_all(path.join(&relative).parent().unwrap()).unwrap();
                    fs::copy(source.join(&relative), path.join(&relative)).unwrap();
                }
            }
        }
        let identity = RootIdentity {
            workspace: WorkspaceId::new(),
            root: RootId::parse(catalog::ROOT_ID).unwrap(),
            repository: "fixture".into(),
            worktree: "fixture".into(),
            binding: Revision::ZERO,
        };
        let root = Root::open(identity.clone(), path).unwrap();
        let registry = SourceRegistry {
            version: 1,
            revision: Revision::ZERO,
            disabled: BTreeSet::new(),
            sources: vec![SkillSource {
                id: catalog::SOURCE_ID.into(),
                kind: SourceKind::Builtin,
                enabled: true,
                root: identity,
                path: path.to_owned(),
            }],
        };
        (root, registry)
    }
    fn context() -> discovery::MatchContext {
        discovery::MatchContext {
            environment: "windows".into(),
            tools: BTreeSet::from(["vcp_list".into(), "vcp_read".into()]),
            cues: BTreeSet::new(),
        }
    }
    #[test]
    fn metadata_verification_and_discovery_never_require_body_files() {
        let temp = tempfile::tempdir().unwrap();
        let (root, registry) = staged(temp.path(), false);
        let verified = catalog::verify(&root).unwrap();
        assert_eq!(verified.reads.metadata_files, 2);
        assert_eq!(verified.reads.descriptors, 21);
        assert!(verified.reads.metadata_bytes > 0 && verified.reads.descriptor_bytes > 0);
        let discovered = discovery::discover(&registry, &Default::default()).unwrap();
        catalog::verify_discovery(&verified, &discovered).unwrap();
        assert_eq!(discovered.reads.bodies, 0);
        assert_eq!(discovered.reads.resources, 0);
        assert_eq!(
            catalog::revalidate(&root, &verified).unwrap().revalidations,
            23
        );
        assert!(activation::activate(
            &registry,
            &discovered,
            "architecture",
            &context(),
            "fixture",
            &Default::default()
        )
        .is_err());
    }
    #[test]
    fn installed_metadata_changes_and_unlisted_descriptors_fail_closed() {
        for path in ["catalog.json", "coverage.json", "architecture/skill.json"] {
            let temp = tempfile::tempdir().unwrap();
            let (root, _) = staged(temp.path(), false);
            let verified = catalog::verify(&root).unwrap();
            fs::write(temp.path().join(path), b"{}").unwrap();
            assert!(catalog::verify(&root).is_err(), "{path}");
            assert!(catalog::revalidate(&root, &verified).is_err(), "{path}");
        }
        let temp = tempfile::tempdir().unwrap();
        let (root, registry) = staged(temp.path(), false);
        let verified = catalog::verify(&root).unwrap();
        fs::create_dir(temp.path().join("unlisted")).unwrap();
        let mut descriptor: SkillDescriptor =
            serde_json::from_slice(&fs::read(temp.path().join("architecture/skill.json")).unwrap())
                .unwrap();
        descriptor.id = "unlisted".into();
        fs::write(
            temp.path().join("unlisted/skill.json"),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let discovered = discovery::discover(&registry, &Default::default()).unwrap();
        assert!(catalog::verify_discovery(&verified, &discovered).is_err());
    }
    #[test]
    fn shipped_activation_preserves_body_hashes_and_disabled_semantics() {
        let temp = tempfile::tempdir().unwrap();
        let (root, mut registry) = staged(temp.path(), true);
        let verified = catalog::verify(&root).unwrap();
        let discovered = discovery::discover(&registry, &Default::default()).unwrap();
        catalog::verify_discovery(&verified, &discovered).unwrap();
        let active = activation::activate(
            &registry,
            &discovered,
            "architecture",
            &context(),
            "explicit fixture request",
            &Default::default(),
        )
        .unwrap();
        assert_eq!(
            active.body.version.sha256,
            verified
                .manifest
                .skills
                .iter()
                .find(|entry| entry.id == "architecture")
                .unwrap()
                .body
                .sha256
        );
        registry.disabled.insert(active.qualified_id.clone());
        assert!(activation::revalidate(&registry, &active).is_err());
        registry.disabled.clear();
        fs::write(
            temp.path().join("architecture/SKILL.md"),
            b"changed instructions",
        )
        .unwrap();
        assert!(activation::activate(
            &registry,
            &discovered,
            "architecture",
            &context(),
            "fixture",
            &Default::default()
        )
        .is_err());
        assert!(activation::revalidate(&registry, &active).is_err());
        // Body edits do not imply hidden body reads during metadata-only verification.
        assert!(catalog::verify(&root).is_ok());
    }
}
