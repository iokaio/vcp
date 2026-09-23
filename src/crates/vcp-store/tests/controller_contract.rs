// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{controller::*, *};
use vcp_store::{contract::*, BackendKind, Store};

fn lease() -> Lease {
    Lease {
        document_type: DOCUMENT_TYPE.into(),
        schema_version: 1,
        id: controller_lease_id(&workspace().id, &session().id).unwrap(),
        workspace: workspace().id,
        session: session().id,
        revision: Revision::ZERO,
        generation: Revision::new(1),
        holder: Some(Holder {
            actor: ActorId::parse("owner").unwrap(),
            connection: ControllerId::parse("connection").unwrap(),
            process_owner: ControllerId::parse("process").unwrap(),
            owner_epoch: OwnerEpoch::new(1),
        }),
        reason: Reason::Acquired,
    }
}

fn record(value: &Lease) -> Record {
    Record::typed(
        Collection::Access,
        &value.id,
        value.workspace.clone(),
        value.revision,
        value,
    )
    .unwrap()
}

fn unchecked_record(value: &Lease) -> Record {
    Record {
        collection: Collection::Access,
        id: value.id.clone(),
        workspace: value.workspace.clone(),
        revision: value.revision,
        value: serde_json::to_value(value).unwrap(),
        references: Default::default(),
    }
}

fn change(state: &State, record: Record, expected: Option<Revision>) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: vec![Mutation::Put { expected, record }],
        events: vec![],
        command: None,
    }
}

fn initial_state() -> State {
    State::default().prepare(&initial()).unwrap().0
}

#[test]
fn bounded_deterministic_key_separates_workspace_and_session() {
    let workspace = WorkspaceId::parse("w".repeat(96)).unwrap();
    let session = SessionId::parse("s".repeat(96)).unwrap();
    let id = controller_lease_id(&workspace, &session).unwrap();
    assert_eq!(id.len(), 75);
    assert!(id.starts_with("controller-"));
    assert!(TaskId::parse(id.clone()).is_ok());
    assert_eq!(controller_lease_id(&workspace, &session).unwrap(), id);
    assert_ne!(
        controller_lease_id(&WorkspaceId::new(), &session).unwrap(),
        id
    );
    assert_ne!(
        controller_lease_id(&workspace, &SessionId::new()).unwrap(),
        id
    );
}

#[test]
fn malformed_lease_records_cannot_hide_in_generic_access_documents() {
    let valid = record(&lease());
    let corruptions: &[fn(&mut Record)] = &[
        |row| row.collection = Collection::Projection,
        |row| row.id = "wrong-key".into(),
        |row| row.workspace = WorkspaceId::new(),
        |row| row.revision = Revision::new(1),
        |row| row.value["document_type"] = "vcp_controller_lease_v2".into(),
        |row| row.value["schema_version"] = 2.into(),
        |row| row.value["session"] = "other-session".into(),
        |row| row.value["holder"] = serde_json::Value::Null,
        |row| row.value["holder"]["owner_epoch"] = "0".into(),
        |row| row.value["generation"] = "0".into(),
        |row| row.value["generation"] = "2".into(),
        |row| row.value["reason"] = "expired".into(),
        |row| row.value["unknown_authority"] = true.into(),
    ];
    for corrupt in corruptions {
        let mut row = valid.clone();
        corrupt(&mut row);
        assert!(
            row.validate_shape().is_err(),
            "accepted malformed lease: {row:?}"
        );
    }
}

#[test]
fn session_reference_is_mandatory_and_workspace_scoped() {
    let state = initial_state();
    let mut value = lease();
    value.session = SessionId::parse("absent").unwrap();
    value.id = controller_lease_id(&value.workspace, &value.session).unwrap();
    let row = record(&value);
    assert!(row
        .required_references()
        .unwrap()
        .contains(&key(Collection::Session, "absent")));
    assert!(state.prepare(&change(&state, row, None)).is_err());

    let mut state = state;
    let mut foreign_workspace = workspace();
    foreign_workspace.id = WorkspaceId::parse("foreign-workspace").unwrap();
    let mut foreign_session = session();
    foreign_session.id = value.session.clone();
    foreign_session.workspace = foreign_workspace.id.clone();
    for row in [
        Record::typed(
            Collection::Workspace,
            foreign_workspace.id.as_str(),
            foreign_workspace.id.clone(),
            Revision::ZERO,
            &foreign_workspace,
        )
        .unwrap(),
        Record::typed(
            Collection::Session,
            foreign_session.id.as_str(),
            foreign_workspace.id,
            Revision::ZERO,
            &foreign_session,
        )
        .unwrap(),
    ] {
        state.records.insert(row.key(), row);
    }
    state.validate().unwrap();
    assert!(state
        .prepare(&change(&state, record(&value), None))
        .is_err());
}

#[test]
fn lease_kind_cannot_be_replaced_with_or_over_generic_access_at_same_key() {
    let initial = initial_state();
    let value = lease();
    let (leased, _) = initial
        .prepare(&change(&initial, record(&value), None))
        .unwrap();
    assert!(Record::typed(
        Collection::Access,
        &value.id,
        value.workspace.clone(),
        Revision::ZERO,
        &serde_json::json!({"schema_version":1,"legacy":true}),
    )
    .is_err());
    let mut generic = Record::typed(
        Collection::Access,
        "legacy-access",
        value.workspace.clone(),
        Revision::new(1),
        &serde_json::json!({"schema_version":1,"legacy":true}),
    )
    .unwrap();
    generic.id = value.id.clone();
    let mut initial_squatting = generic.clone();
    initial_squatting.revision = Revision::ZERO;
    assert!(initial
        .prepare(&change(&initial, initial_squatting, None))
        .is_err());
    assert!(leased
        .prepare(&change(&leased, generic.clone(), Some(Revision::ZERO)))
        .is_err());

    // Simulate an old/corrupt reserved-key occupant without passing current
    // constructors. It cannot be silently upgraded into controller authority.
    let mut legacy = initial.clone();
    legacy.records.insert(generic.key(), generic);
    let mut replacement = value;
    replacement.revision = Revision::new(2);
    replacement.generation = Revision::new(2);
    assert!(legacy
        .prepare(&change(
            &legacy,
            record(&replacement),
            Some(Revision::new(1))
        ))
        .is_err());
    assert!(legacy.validate().is_err());
    // Generic documents outside the exact new namespace remain compatible.
    let ordinary = Record::typed(
        Collection::Access,
        "legacy-access",
        replacement.workspace.clone(),
        Revision::ZERO,
        &serde_json::json!({"schema_version":1,"legacy":true}),
    )
    .unwrap();
    let (compatible, _) = initial.prepare(&change(&initial, ordinary, None)).unwrap();
    compatible.validate().unwrap();
    for name in [
        format!("controller-{}", "A".repeat(64)),
        format!("controller-{}", "a".repeat(63)),
        format!("controller-{}", "a".repeat(65)),
        "controller-legacy".into(),
    ] {
        assert!(Record::typed(
            Collection::Access,
            name,
            replacement.workspace.clone(),
            Revision::ZERO,
            &serde_json::json!({"schema_version":1,"legacy":true}),
        )
        .is_ok());
    }
}

#[test]
fn generation_changes_only_on_acquisition_and_held_leases_require_release() {
    let before = lease();
    let mut released = before.clone();
    released.revision = Revision::new(1);
    released.reason = Reason::ConnectionLost;
    released.holder = None;
    before.validate_transition(&released).unwrap();
    let mut acquired = before.clone();
    acquired.revision = Revision::new(2);
    acquired.generation = Revision::new(2);
    released.validate_transition(&acquired).unwrap();
    let mut takeover = before.clone();
    takeover.revision = Revision::new(1);
    takeover.generation = Revision::new(2);
    takeover.holder.as_mut().unwrap().connection = ControllerId::new();
    assert!(before.validate_transition(&takeover).is_err());
    let mut bad_release = released.clone();
    bad_release.generation = Revision::new(2);
    assert!(before.validate_transition(&bad_release).is_err());
    let mut stale_acquire = acquired.clone();
    stale_acquire.generation = Revision::new(1);
    assert!(released.validate_transition(&stale_acquire).is_err());
    let mut repeated_release = released.clone();
    repeated_release.revision = Revision::new(2);
    assert!(released.validate_transition(&repeated_release).is_err());
    let mut maximum_held = before.clone();
    maximum_held.revision = Revision::new(u64::MAX - 1);
    maximum_held.generation = Revision::new(1_u64 << 63);
    maximum_held.validate().unwrap();
    let mut maximum_released = maximum_held.clone();
    maximum_released.holder = None;
    maximum_released.reason = Reason::Released;
    maximum_released.revision = Revision::new(u64::MAX);
    maximum_held.validate_transition(&maximum_released).unwrap();
    assert_eq!(
        maximum_released.validate_transition(&before),
        Err(vcp_domain::Error::Overflow)
    );
    let bytes = serde_json::to_string(&maximum_released).unwrap();
    assert!(bytes.contains("\"revision\":\"18446744073709551615\""));
    assert_eq!(
        serde_json::from_str::<Lease>(&bytes).unwrap(),
        maximum_released
    );
    for generation in [(1_u64 << 63) + 1, u64::MAX] {
        let mut impossible = maximum_held.clone();
        impossible.generation = Revision::new(generation);
        assert!(impossible.validate().is_err());
    }
    for (generation, revision, held) in [(1, 2, true), (2, 3, true), (2, 2, false), (1, 4, false)] {
        let mut impossible = if held {
            before.clone()
        } else {
            released.clone()
        };
        impossible.generation = Revision::new(generation);
        impossible.revision = Revision::new(revision);
        assert!(impossible.validate().is_err());
    }
}

#[test]
fn replay_rejects_illegal_lease_transition_even_with_matching_transaction_digest() {
    let initial = initial_state();
    let value = lease();
    let (mut state, _) = initial
        .prepare(&change(&initial, record(&value), None))
        .unwrap();
    let mut released = value.clone();
    released.revision = Revision::new(1);
    released.reason = Reason::Released;
    released.holder = None;
    let (_, mut commit) = state
        .prepare(&change(&state, record(&released), Some(Revision::ZERO)))
        .unwrap();
    let mut takeover = value;
    takeover.revision = Revision::new(1);
    takeover.generation = Revision::new(2);
    takeover.holder.as_mut().unwrap().connection = ControllerId::new();
    commit.transaction.mutations[0] = Mutation::Put {
        expected: Some(Revision::ZERO),
        record: unchecked_record(&takeover),
    };
    commit.receipt.digest =
        vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&commit.transaction).unwrap());
    let before = state.clone();
    assert!(matches!(
        state.replay(&commit),
        Err(vcp_store::Error::Domain(vcp_domain::Error::Invalid(
            "controller lease revision and generation"
        )))
    ));
    assert_eq!(state, before);
}

#[tokio::test]
async fn both_backends_retain_release_generation_and_reject_invalid_replacement_atomically() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        store.transact(initial()).await.unwrap();
        let value = lease();
        store
            .transact(change(store.state(), record(&value), None))
            .await
            .unwrap();
        let mut released = value.clone();
        released.holder = None;
        released.reason = Reason::ProcessLost;
        released.revision = Revision::new(1);
        store
            .transact(change(
                store.state(),
                record(&released),
                Some(Revision::ZERO),
            ))
            .await
            .unwrap();
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let retained: Lease = store
            .state()
            .record(Collection::Access, &value.id, &value.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(retained, released);
        let mut next = value;
        next.revision = Revision::new(2);
        next.generation = Revision::new(2);
        next.holder.as_mut().unwrap().process_owner = ControllerId::new();
        store
            .transact(change(
                store.state(),
                record(&next),
                Some(released.revision),
            ))
            .await
            .unwrap();
        let saved = store.state().clone();
        let mut invalid = next.clone();
        invalid.revision = Revision::new(3);
        invalid.holder.as_mut().unwrap().connection = ControllerId::new();
        assert!(store
            .transact(change(
                store.state(),
                unchecked_record(&invalid),
                Some(next.revision)
            ))
            .await
            .is_err());
        assert_eq!(store.state(), &saved);
        store.close().await.unwrap();
        let store = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!(store.state(), &saved);
        store.close().await.unwrap();
    }
}
