// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{ids::*, revision::*};
use vcp_protocol::{canonical_bytes, command::*, version::*};

#[test]
fn canonical_objects_ignore_key_order_but_keep_array_and_revision_meaning() {
    let a: serde_json::Value = serde_json::from_str(r#"{"b":{"y":2,"x":1},"a":[1,2]}"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"{"a":[1,2],"b":{"x":1,"y":2}}"#).unwrap();
    assert_eq!(canonical_bytes(&a).unwrap(), canonical_bytes(&b).unwrap());
    let changed: serde_json::Value =
        serde_json::from_str(r#"{"a":[2,1],"b":{"x":1,"y":2}}"#).unwrap();
    assert_ne!(
        canonical_bytes(&a).unwrap(),
        canonical_bytes(&changed).unwrap()
    );
}
#[test]
fn envelopes_reject_unknown_versions_fields_and_oversized_input() {
    let command = CommandEnvelope {
        version: VERSION,
        id: CommandId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: None,
        caller: ActorId::new(),
        controller: ControllerId::new(),
        owner_epoch: OwnerEpoch::new(1),
        expected: Revision::new(9_007_199_254_740_993),
        steering: SteeringRevision::ZERO,
        payload: Command::Inspect,
    };
    let bytes = serde_json::to_vec(&command).unwrap();
    assert_eq!(CommandEnvelope::parse_jsonl(&bytes).unwrap(), command);
    let mut value = serde_json::to_value(&command).unwrap();
    value["version"] = 999.into();
    assert!(matches!(
        CommandEnvelope::parse_jsonl(&serde_json::to_vec(&value).unwrap()),
        Err(Error::Version(999))
    ));
    value["version"] = 1.into();
    value["authority"] = true.into();
    assert!(CommandEnvelope::parse_jsonl(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(matches!(
        CommandEnvelope::parse_jsonl(&vec![b' '; MAX_COMMAND_BYTES + 1]),
        Err(Error::Limit)
    ));
}
