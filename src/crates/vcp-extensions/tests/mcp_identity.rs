use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{policy::GrantScope, Revision, WorkspaceId};
use vcp_extensions::mcp::{
    identity::{ConnectionIdentity, ToolIdentity, PROTOCOL},
    registration::{Capabilities, Limits, Registration, Transport},
    schema::{self, Schema},
};

fn registration() -> Registration {
    Registration {
        id: "fixture".into(),
        revision: Revision::ZERO,
        scope: GrantScope::Workspace {
            workspace: WorkspaceId::parse("workspace").unwrap(),
        },
        transport: Transport::Stdio {
            process_profile: "trusted-profile".into(),
            resolved_digest: "0".repeat(64),
        },
        auth_refs: BTreeSet::new(),
        allowed_tools: BTreeSet::from(["echo".into()]),
        trusted_effects: BTreeMap::new(),
        limits: Limits {
            frame_bytes: 1024,
            total_discovery_bytes: 4096,
            tools: 16,
            pages: 4,
            timeout_ms: 1000,
            stderr_bytes: 1024,
        },
        capabilities: Capabilities::default(),
    }
}

#[test]
fn reconnect_and_reopened_registration_never_restore_generation() {
    let registration = registration();
    let schema = Schema::compile(br#"{"type":"object"}"#, schema::Limits::default()).unwrap();
    let before = ConnectionIdentity::new(&registration, PROTOCOL, None).unwrap();
    let reopened: Registration =
        serde_json::from_slice(&serde_json::to_vec(&registration).unwrap()).unwrap();
    let after = ConnectionIdentity::new(&reopened, PROTOCOL, None).unwrap();
    assert_eq!(before.registration_digest(), after.registration_digest());
    assert_ne!(before.generation(), after.generation());
    assert_eq!(before.generation().len(), 36);
    let a = ToolIdentity::new(before, "echo".into(), &schema).unwrap();
    let b = ToolIdentity::new(after, "echo".into(), &schema).unwrap();
    assert_ne!(a.digest().unwrap(), b.digest().unwrap());
    // A caller cannot provide or deserialize generation into checked identities.
    assert!(ConnectionIdentity::new(&registration, "2026-07-28", None).is_err());
    assert!(
        ConnectionIdentity::new(&registration, PROTOCOL, Some("unverified peer name".into()))
            .is_err()
    );
}

#[test]
fn trusted_effects_default_opaque_and_optional_capabilities_fail_closed() {
    let mut registration = registration();
    assert_eq!(
        registration.effects("echo").unwrap(),
        BTreeSet::from([vcp_domain::policy::EffectClass::Opaque])
    );
    assert!(registration.effects("not-allowed").is_err());
    registration.capabilities.sampling = true;
    assert!(registration.validate().is_err());
}
