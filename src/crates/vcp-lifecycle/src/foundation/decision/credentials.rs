// SPDX-License-Identifier: Apache-2.0
//! Independently scoped evaluator credentials; possession never grants dispatch.
use std::{
    fmt,
    sync::{Arc, Mutex},
};
use vcp_domain::{
    ids::{ControllerId, WorkspaceId},
    revision::{OwnerEpoch, Revision, Timestamp},
};
use vcp_models::decision::Operation;
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub workspace: WorkspaceId,
    pub owner: OwnerEpoch,
    pub controller: ControllerId,
    pub config_digest: String,
    pub operation: Operation,
    pub fixture: bool,
}
pub struct CredentialMaterial(Zeroizing<String>);
impl CredentialMaterial {
    pub fn bearer(value: String) -> Result<Self, String> {
        let value = Zeroizing::new(value);
        if value.is_empty()
            || value.len() > 8192
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
        {
            return Err("invalid evaluator credential".into());
        }
        Ok(Self(value))
    }
}
impl CredentialMaterial {
    pub(crate) fn contains_secret(&self, bytes: &[u8]) -> Result<bool, String> {
        self.filter()?
            .contains_secret(bytes)
            .map_err(|_| "invalid evaluator capture".into())
    }
    pub(crate) fn sanitize_json(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        // Decisions allow numeric tokens beyond the MCP numeric profile. Check
        // literal bytes before serde can rewrite their exponent spelling.
        if self.contains_secret(bytes)? {
            return Err("credential-bearing evaluator capture".into());
        }
        self.filter()?
            .sanitize_json(bytes)
            .map_err(|_| "invalid evaluator capture".into())
    }
    fn filter(&self) -> Result<super::super::mcp::remote_authority::CaptureSecret<'_>, String> {
        // Shared capture filter only: MCP registration/session authority is unused.
        super::super::mcp::remote_authority::CaptureSecret::new(self.0.as_str())
            .map_err(|_| "invalid evaluator capture".into())
    }
}
impl fmt::Debug for CredentialMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialMaterial([omitted])")
    }
}
#[derive(Default)]
struct State {
    closed: bool,
    revision: Option<Revision>,
    entry: Option<Entry>,
}
struct Entry {
    pin: Pin,
    expires_at: Timestamp,
    material: Arc<CredentialMaterial>,
}
#[derive(Clone, Default)]
pub struct Registry(Arc<Mutex<State>>);
impl Registry {
    pub fn install(
        &self,
        pin: Pin,
        expected: Option<Revision>,
        material: CredentialMaterial,
        expires_at: Timestamp,
        now: Timestamp,
    ) -> Result<CredentialLease, String> {
        if expires_at <= now
            || pin.config_digest.len() != 64
            || !pin
                .config_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || pin.fixture && !cfg!(any(test, feature = "qualification"))
        {
            return Err("invalid evaluator credential scope".into());
        }
        let mut state = self
            .0
            .lock()
            .map_err(|_| "evaluator credentials unavailable")?;
        if state.closed || state.revision != expected {
            return Err("stale evaluator credential installation".into());
        }
        let revision = match expected {
            Some(value) => value
                .next()
                .map_err(|_| "evaluator credential revision exhausted")?,
            None => Revision::ZERO,
        };
        let material = Arc::new(material);
        state.revision = Some(revision);
        state.entry = Some(Entry {
            pin: pin.clone(),
            expires_at,
            material: material.clone(),
        });
        Ok(CredentialLease {
            state: self.0.clone(),
            pin,
            revision,
            expires_at,
            material,
        })
    }
    pub fn resolve(&self, pin: &Pin, now: Timestamp) -> Result<CredentialLease, String> {
        let state = self
            .0
            .lock()
            .map_err(|_| "evaluator credentials unavailable")?;
        let entry = state.entry.as_ref().ok_or("evaluator credential absent")?;
        if state.closed || &entry.pin != pin || entry.expires_at <= now {
            return Err("evaluator credential unavailable".into());
        }
        Ok(CredentialLease {
            state: self.0.clone(),
            pin: pin.clone(),
            revision: state.revision.ok_or("evaluator credential absent")?,
            expires_at: entry.expires_at,
            material: entry.material.clone(),
        })
    }
    pub fn revoke(&self, expected: Revision) -> Result<Revision, String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "evaluator credentials unavailable")?;
        if state.closed || state.revision != Some(expected) {
            return Err("stale evaluator credential revocation".into());
        }
        let next = expected
            .next()
            .map_err(|_| "evaluator credential revision exhausted")?;
        state.revision = Some(next);
        state.entry = None;
        Ok(next)
    }
    pub fn close(&self) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "evaluator credentials unavailable")?;
        state.closed = true;
        state.entry = None;
        Ok(())
    }
}
#[derive(Clone)]
pub struct CredentialLease {
    state: Arc<Mutex<State>>,
    pin: Pin,
    revision: Revision,
    expires_at: Timestamp,
    material: Arc<CredentialMaterial>,
}
impl fmt::Debug for CredentialLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialLease([omitted])")
    }
}
impl CredentialLease {
    pub fn revision(&self) -> Revision {
        self.revision
    }
    pub fn expires_at(&self) -> Timestamp {
        self.expires_at
    }
    pub(crate) fn pin(&self) -> &Pin {
        &self.pin
    }
    pub fn validate(&self, pin: &Pin, now: Timestamp) -> Result<(), String> {
        if pin != &self.pin {
            return Err("evaluator credential binding changed".into());
        }
        self.with_current(now, || ())
    }
    pub(crate) fn with_current<T>(
        &self,
        now: Timestamp,
        action: impl FnOnce() -> T,
    ) -> Result<T, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "evaluator credentials unavailable")?;
        let entry = state.entry.as_ref().ok_or("evaluator credential revoked")?;
        if state.closed
            || state.revision != Some(self.revision)
            || entry.pin != self.pin
            || now >= entry.expires_at
        {
            return Err("evaluator credential revoked or expired".into());
        }
        Ok(action())
    }
    pub(crate) fn with_bearer<T>(
        &self,
        now: Timestamp,
        action: impl FnOnce(&str) -> T,
    ) -> Result<T, String> {
        self.with_current(now, || action(self.material.0.as_str()))
    }
    pub fn contains_secret(&self, bytes: &[u8]) -> Result<bool, String> {
        self.material.contains_secret(bytes)
    }
    pub fn sanitize_json(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        self.material.sanitize_json(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pin() -> Pin {
        Pin {
            workspace: WorkspaceId::new(),
            owner: OwnerEpoch::new(1),
            controller: ControllerId::new(),
            config_digest: "a".repeat(64),
            operation: Operation::JevDecisions,
            fixture: true,
        }
    }
    fn material() -> CredentialMaterial {
        CredentialMaterial::bearer("synthetic-decision-canary".into()).unwrap()
    }
    #[test]
    fn decision_credential_scope_rotation_revocation_expiry_and_close() {
        let registry = Registry::default();
        let pin = pin();
        let lease = registry
            .install(
                pin.clone(),
                None,
                material(),
                Timestamp::new(20),
                Timestamp::new(1),
            )
            .unwrap();
        assert!(lease.validate(&pin, Timestamp::new(19)).is_ok());
        assert!(lease.validate(&pin, Timestamp::new(20)).is_err());
        let mut changed = pin.clone();
        changed.operation = Operation::ConventionalChat;
        assert!(registry.resolve(&changed, Timestamp::new(1)).is_err());
        changed = pin.clone();
        changed.owner = OwnerEpoch::new(2);
        assert!(lease.validate(&changed, Timestamp::new(1)).is_err());
        let replacement = registry
            .install(
                pin.clone(),
                Some(lease.revision()),
                material(),
                Timestamp::new(30),
                Timestamp::new(1),
            )
            .unwrap();
        assert!(lease
            .with_current(Timestamp::new(1), || panic!("revoked bytes executed"))
            .is_err());
        let revision = registry.revoke(replacement.revision()).unwrap();
        assert!(replacement
            .with_current(Timestamp::new(1), || panic!("revoked bytes executed"))
            .is_err());
        let final_lease = registry
            .install(
                pin,
                Some(revision),
                material(),
                Timestamp::new(30),
                Timestamp::new(1),
            )
            .unwrap();
        registry.close().unwrap();
        assert!(final_lease
            .with_current(Timestamp::new(1), || panic!("closed bytes executed"))
            .is_err());
    }
    #[test]
    fn decision_credential_capture_escapes_and_debug_remain_private() {
        let registry = Registry::default();
        let lease = registry
            .install(
                pin(),
                None,
                material(),
                Timestamp::new(20),
                Timestamp::new(1),
            )
            .unwrap();
        assert!(!format!("{lease:?}").contains("canary"));
        registry.close().unwrap(); // Late evidence can still be sanitized after revocation.
        let safe = lease
            .sanitize_json(
                br#"{"echo":"synthetic-decision-\u0063anary","cost":0.0000120000000000000001}"#,
            )
            .unwrap();
        assert!(!String::from_utf8_lossy(&safe).contains("canary"));
        assert!(String::from_utf8_lossy(&safe).contains("0.0000120000000000000001"));
        assert!(lease
            .sanitize_json(br#"{"synthetic-decision-canary":1}"#)
            .is_err());
        assert!(CredentialMaterial::bearer("secret\r\nInjected:1".into()).is_err());
    }
    #[test]
    fn decision_out_of_profile_numeric_credential_cannot_transform_into_capture() {
        let registry = Registry::default();
        let lease = registry
            .install(
                pin(),
                None,
                CredentialMaterial::bearer("1e999".into()).unwrap(),
                Timestamp::new(20),
                Timestamp::new(1),
            )
            .unwrap();
        assert!(lease.sanitize_json(br#"{"cost":1e999}"#).is_err());
        let safe = lease.sanitize_json(br#"{"text":"1\u0065999"}"#).unwrap();
        assert!(!String::from_utf8_lossy(&safe).contains("1e999"));
        assert!(!String::from_utf8_lossy(&safe).contains("1e+999"));
    }
}
