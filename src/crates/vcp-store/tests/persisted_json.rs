// SPDX-License-Identifier: Apache-2.0
//! Canonical v1 literal objects must survive every persisted-history decoder.
mod common;
use common::*;
use serde_json::{Map, Value};
use vcp_domain::*;
use vcp_protocol::canonical_bytes;
use vcp_store::{contract::*, portable_snapshot::Archive, BackendKind, Store};

const NUMBER: &str = "$serde_json::private::Number";
const RAW: &str = "$serde_json::private::RawValue";

fn object(entries: impl IntoIterator<Item = (String, Value)>) -> Value {
    Value::Object(entries.into_iter().collect::<Map<_, _>>())
}
fn payload() -> Value {
    let escaped: String = serde_json::from_str(r#""\u0024serde_json::private::Number""#).unwrap();
    let literal = |key: &str, text: &str| object([(key.into(), Value::String(text.into()))]);
    object([
        ("schema_version".into(), 1.into()),
        ("number_object".into(), literal(NUMBER, "1.5")),
        ("raw_object".into(), literal(RAW, r#"{"external":true}"#)),
        (
            "escaped_key".into(),
            literal(&escaped, "18446744073709551616"),
        ),
        (
            "siblings".into(),
            object([
                (NUMBER.into(), "0.1".into()),
                ("ordinary".into(), true.into()),
            ]),
        ),
        (
            "both".into(),
            object([(NUMBER.into(), "3.5".into()), (RAW.into(), "null".into())]),
        ),
        (
            "nested".into(),
            Value::Array(vec![
                literal(NUMBER, "not a numeric token"),
                literal(RAW, "not JSON"),
            ]),
        ),
        (
            "exact".into(),
            Value::Array(
                [
                    "9007199254740993",
                    "18446744073709551616",
                    "0.12345678901234567890123456789",
                    "3e-128",
                ]
                .into_iter()
                .map(|token| serde_json::from_str(token).unwrap())
                .collect(),
            ),
        ),
    ])
}
fn transaction() -> Transaction {
    let mut transaction = initial();
    transaction.events[0].data = payload();
    transaction.mutations.push(Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Projection,
            "literal-data",
            workspace().id,
            Revision::ZERO,
            &payload(),
        )
        .unwrap(),
    });
    transaction
}
fn unchanged(state: &State, bytes: &[u8]) {
    assert_eq!(canonical_bytes(state).unwrap(), bytes);
    assert_eq!(
        state
            .record(Collection::Projection, "literal-data", &workspace().id)
            .unwrap()
            .value,
        payload()
    );
    assert_eq!(state.events[0].event.data, payload());
}

fn typed_round_trip<
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
>(
    value: &T,
) {
    let bytes = canonical_bytes(value).unwrap();
    assert_eq!(&serde_json::from_slice::<T>(&bytes).unwrap(), value);
    let tree = serde_json::to_value(value).unwrap();
    let owned: T = serde_json::from_value(tree.clone()).unwrap();
    assert_eq!(&owned, value);
    assert_eq!(&T::deserialize(&tree).unwrap(), value);
    assert_eq!(canonical_bytes(&owned).unwrap(), bytes);
}

#[test]
fn typed_store_conversions_preserve_literals_and_mutation_shapes_stay_closed() {
    let transaction = transaction();
    for mutation in &transaction.mutations {
        typed_round_trip(mutation);
        if let Mutation::Put { record, .. } = mutation {
            typed_round_trip(record);
            if record.id == "literal-data" {
                // Arbitrary JSON is exposed directly by value. decode<T> is a
                // typed-contract serde conversion; generic Value deserialization
                // has its own pre-existing private-map semantics and no caller
                // in production. This fix preserves the durable envelope.
                assert_eq!(record.value, payload());
            }
        }
    }
    typed_round_trip(&transaction.events[0]);
    typed_round_trip(&transaction);
    let (state, commit) = State::default().prepare(&transaction).unwrap();
    typed_round_trip(&state);
    typed_round_trip(&commit);
    typed_round_trip(&state.events[0]);
    let drop = Mutation::DropProjection {
        id: "literal-data".into(),
        expected: Revision::ZERO,
    };
    typed_round_trip(&drop);
    for raw in [
        r#"{"operation":"drop_projection","id":"x","expected":"1","record":{}}"#,
        r#"{"operation":"drop_projection","operation":"put","id":"x","expected":"1"}"#,
        r#"{"operation":"drop_projection","id":"x","id":"y","expected":"1"}"#,
        r#"{"operation":"unknown","id":"x","expected":"1"}"#,
        r#"{"operation":"put","record":{},"extra":true}"#,
        r#"{"operation":"drop_projection","id":"x"}"#,
    ] {
        assert!(serde_json::from_str::<Mutation>(raw).is_err(), "{raw}");
    }
}

#[test]
fn canonical_v1_literal_spelling_is_unchanged() {
    // Independent fixed bytes, rather than a second decode through the new helper.
    let value = object([(NUMBER.into(), "1.5".into()), (RAW.into(), "null".into())]);
    assert_eq!(
        canonical_bytes(&value).unwrap(),
        br#"{"$serde_json::private::Number":"1.5","$serde_json::private::RawValue":"null"}"#
    );
    assert_eq!(
        canonical_bytes(&serde_json::from_str::<Value>("9007199254740993").unwrap()).unwrap(),
        b"9007199254740993"
    );
}

#[tokio::test]
async fn literal_record_and_event_replay_checkpoint_and_retry_preserve_exact_bytes() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let transaction = transaction();
        let receipt = store.transact(transaction.clone()).await.unwrap();
        let expected = canonical_bytes(store.state()).unwrap();
        unchanged(store.state(), &expected);
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        unchanged(store.state(), &expected);
        assert_eq!(store.transact(transaction.clone()).await.unwrap(), receipt);
        // Files reads its serialized State checkpoint on the next open.
        store.checkpoint().unwrap();
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        unchanged(store.state(), &expected);
        assert_eq!(store.transact(transaction).await.unwrap(), receipt);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn literal_payloads_survive_replay_base_suffix_and_backend_conversion() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let transaction = transaction();
        let receipt = store.transact(transaction.clone()).await.unwrap();
        let expected = canonical_bytes(store.state()).unwrap();
        store
            .rewrite_base(store.state().clone(), &[])
            .await
            .unwrap();
        unchanged(store.state(), &expected);
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        unchanged(store.state(), &expected);
        assert_eq!(store.transact(transaction).await.unwrap(), receipt);
        let mut suffix = initial();
        suffix.id = TransactionId::parse("literal-suffix").unwrap();
        suffix.expected_watermark = store.state().watermark;
        suffix.command = None;
        suffix.events.clear();
        suffix.mutations = vec![Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Projection,
                "literal-suffix",
                workspace().id,
                Revision::ZERO,
                &payload(),
            )
            .unwrap(),
        }];
        let suffix_receipt = store.transact(suffix.clone()).await.unwrap();
        let expected = canonical_bytes(store.state()).unwrap();
        store.checkpoint().unwrap();
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        unchanged(store.state(), &expected);
        assert_eq!(store.transact(suffix).await.unwrap(), suffix_receipt);
        let other = if backend == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let converted_root = temp.path().join("converted");
        let converted = store.convert(&converted_root, other, &[]).await.unwrap();
        unchanged(converted.state(), &expected);
        converted.close().await.unwrap();
        let converted = Store::open(&converted_root, other, &[]).await.unwrap();
        unchanged(converted.state(), &expected);
        converted.close().await.unwrap();
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn portable_history_export_decode_and_import_preserve_literal_payloads() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        store.transact(transaction()).await.unwrap();
        let expected = canonical_bytes(store.state()).unwrap();
        let snapshot = store.snapshot().unwrap();
        let archive = Archive::capture(&store, &snapshot, &workspace().id, &|| false).unwrap();
        let payloads = archive.payloads().unwrap();
        let digest = archive.inventory_digest().unwrap();
        let imported = Archive::decode(payloads.clone(), &digest).unwrap();
        unchanged(imported.state(), &expected);
        assert_eq!(imported.payloads().unwrap(), payloads);
        assert_eq!(imported.inventory_digest().unwrap(), digest);
        let parent = temp.path().join("historical-import");
        std::fs::create_dir(&parent).unwrap();
        let staged = imported.stage_history(&parent, &[root], &|| false).unwrap();
        unchanged(staged.state(), &expected);
        assert_eq!(
            std::fs::read(staged.root().join("canonical-history.json")).unwrap(),
            expected
        );
        // This import is explicitly historical; no old authority is activated.
        drop(snapshot);
        store.close().await.unwrap();
    }
}
