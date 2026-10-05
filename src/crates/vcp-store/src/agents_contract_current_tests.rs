// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::CurrentState;
#[path = "../tests/common/mod.rs"]
mod common;
#[path = "agents_contract_reference.rs"]
#[allow(dead_code)]
mod reference;

fn insert<T: serde::Serialize>(state: &mut State, collection: Collection, id: &str, value: &T) {
    let record = Record::typed(
        collection,
        id,
        common::workspace().id,
        Revision::ZERO,
        value,
    )
    .unwrap();
    state.records.insert(record.key(), record);
}
fn fixture() -> (State, State, TaskId, ArtifactId) {
    let mut before = State::default().prepare(&common::initial()).unwrap().0;
    let mut root = common::task();
    root.state = vcp_domain::task::TaskState::Running;
    insert(
        &mut before,
        Collection::Task,
        root.scope.task.as_str(),
        &root,
    );
    let mut after = before.clone();
    let mut child = common::task();
    child.scope.task = TaskId::new();
    child.parent = Some(root.scope.task.clone());
    insert(
        &mut after,
        Collection::Task,
        child.scope.task.as_str(),
        &child,
    );
    let artifact = ArtifactDescriptor {
        spec: common::spec(),
        state: vcp_domain::artifact::CaptureState::Complete,
        length: ByteCount::ZERO,
        sha256: vcp_protocol::digest_bytes(b""),
        retained: vec![vcp_domain::artifact::Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    insert(
        &mut after,
        Collection::Artifact,
        artifact.spec.id.as_str(),
        &artifact,
    );
    let graph = TaskGraph {
        document_type: GRAPH.into(),
        schema_version: 1,
        scope: root.scope.clone(),
        revision: Revision::ZERO,
        limits: GraphLimits::default(),
        ready: Default::default(),
        results: Default::default(),
        cleanup: Default::default(),
        children: BTreeMap::from([(
            child.scope.task.clone(),
            ChildSpec {
                parent: root.scope.task.clone(),
                actor: ActorId::parse("human").unwrap(),
                parent_steering: root.steering,
                dependencies: Default::default(),
                mode: ChildMode::ReadOnly,
                role: "research".into(),
                model_policy: "fixture/model".into(),
                paths: vec![ChildPath {
                    root: RootId::parse("workspace").unwrap(),
                    path: "file.txt".into(),
                    write: false,
                }],
                effects: BTreeSet::from([vcp_domain::policy::EffectClass::Read]),
                authority: AuthorityRevision::ZERO,
                policy: PolicyRevision::ZERO,
                binding: Revision::ZERO,
                grants: Default::default(),
                allocation: Micros::new(10),
                deadline: Timestamp::new(10000),
                snapshot: artifact.spec.id.clone(),
                snapshot_digest: artifact.sha256.clone(),
                registration: None,
                registration_digest: None,
                isolated_root: None,
            },
        )]),
    };
    insert(
        &mut after,
        Collection::Projection,
        &graph_id(&root.scope.task),
        &graph,
    );
    let ledger = Ledger {
        schema_version: 1,
        scope: root.scope.clone(),
        revision: Revision::ZERO,
        policy: PolicyRevision::ZERO,
        currency: vcp_domain::accounting::Currency::try_from("USD".to_owned()).unwrap(),
        cap: Micros::new(100).into(),
        protected: Micros::ZERO,
        settled: Micros::ZERO,
        active: Micros::ZERO.into(),
        unresolved: Micros::ZERO.into(),
        allocations: BTreeMap::from([(child.scope.task.clone(), Micros::new(10))]),
        daily: None,
        overrun: false,
    };
    insert(
        &mut after,
        Collection::Ledger,
        root.scope.task.as_str(),
        &ledger,
    );
    (before, after, child.scope.task, artifact.spec.id)
}

#[test]
fn current_graph_validation_and_publication_match_frozen_reference() {
    let (base, published, child, artifact) = fixture();
    reference::validate(&published).unwrap();
    reference::publication(base.record_view(), &published).unwrap();
    for variant in 0..9 {
        let mut before = base.clone();
        let mut after = published.clone();
        match variant {
            1 => {
                after
                    .records
                    .get_mut(&key(Collection::Task, child.as_str()))
                    .unwrap()
                    .value["state"] = "running".into()
            }
            2 => {
                before
                    .records
                    .get_mut(&key(Collection::Task, common::task().scope.task.as_str()))
                    .unwrap()
                    .value["state"] = "pending".into()
            }
            3 => {
                let row = after
                    .records
                    .get_mut(&key(Collection::Ledger, common::task().scope.task.as_str()))
                    .unwrap();
                let mut ledger: Ledger = row.decode().unwrap();
                ledger.cap = Micros::new(9).into();
                row.value = serde_json::to_value(ledger).unwrap();
            }
            4 => {
                after
                    .records
                    .get_mut(&key(Collection::Task, child.as_str()))
                    .unwrap()
                    .value["parent"] = "other-parent".into()
            }
            5 => {
                after
                    .records
                    .get_mut(&key(Collection::Artifact, artifact.as_str()))
                    .unwrap()
                    .value["sha256"] = "a".repeat(64).into()
            }
            6 => {
                after
                    .records
                    .get_mut(&key(Collection::Task, child.as_str()))
                    .unwrap()
                    .value["scope"]["session"] = "other-session".into()
            }
            7 => {
                before
                    .records
                    .remove(&key(Collection::Task, common::task().scope.task.as_str()));
            }
            8 => {
                before
                    .records
                    .get_mut(&key(Collection::Task, common::task().scope.task.as_str()))
                    .unwrap()
                    .workspace = WorkspaceId::new()
            }
            _ => (),
        }
        let before_current = CurrentState::from_state(&before);
        let after_current = CurrentState::from_state(&after);
        assert_eq!(
            format!("{:?}", validate_current((&after_current).into())),
            format!("{:?}", reference::validate(&after)),
            "validation variant {variant}"
        );
        assert_eq!(
            format!(
                "{:?}",
                publication_current((&before_current).into(), (&after_current).into())
            ),
            format!("{:?}", reference::publication(before.record_view(), &after)),
            "publication variant {variant}"
        );
    }
}
