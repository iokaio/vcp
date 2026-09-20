// SPDX-License-Identifier: Apache-2.0
//! Trusted remote setup and ephemeral credentials only. These values do not grant
//! network authority; the canonical broker must independently admit each send.
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex},
};
use vcp_domain::{
    ids::{ControllerId, WorkspaceId},
    revision::{AuthorityRevision, OwnerEpoch, Revision, Timestamp},
};
use zeroize::Zeroizing;

const MAX_CREDENTIALS: usize = 64;
const MAX_SECRET: usize = 8192;
const MAX_CAPTURE: usize = 16 * 1024 * 1024;
// Matches the maximum admitted MCP frame, bounding serde Value allocation.
const MAX_JSON_FRAME: usize = 1024 * 1024;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProfileConfig {
    pub workspace: WorkspaceId,
    pub server: String,
    pub revision: Revision,
    pub endpoint: String,
    pub credential_ref: Option<String>,
}

/// Validated exact recipient; no redirect, proxy, URL auth or query convention.
#[derive(Clone, Debug)]
pub struct RemoteProfile {
    config: RemoteProfileConfig,
    digest: String,
}
impl RemoteProfile {
    pub fn new(config: RemoteProfileConfig) -> Result<Self, Error> {
        if !identifier(&config.server)
            || config
                .credential_ref
                .as_ref()
                .is_some_and(|id| !identifier(id))
            || config.endpoint.len() > 4096
            || config
                .endpoint
                .bytes()
                .any(|b| b.is_ascii_control() || b == b'\\')
        {
            return Err(Error::InvalidProfile);
        }
        let url = url::Url::parse(&config.endpoint).map_err(|_| Error::InvalidProfile)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.as_str() != config.endpoint
        {
            return Err(Error::InvalidProfile);
        }
        let bytes = vcp_protocol::canonical_bytes(&config).map_err(|_| Error::InvalidProfile)?;
        Ok(Self {
            config,
            digest: vcp_protocol::digest_bytes(&bytes),
        })
    }
    pub fn config(&self) -> &RemoteProfileConfig {
        &self.config
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn endpoint(&self) -> &str {
        &self.config.endpoint
    }
}

/// Capture from the current canonical owner when trusted setup is installed.
/// A serialized pin alone is never evidence that a caller has current authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityPin {
    pub controller: ControllerId,
    pub owner: OwnerEpoch,
    pub authority: AuthorityRevision,
    pub binding: Revision,
}

/// In-memory injection boundary. No environment, file or OS secret discovery.
/// No Serialize/Display and Debug deliberately omits both bytes and length.
pub struct CredentialMaterial(Zeroizing<String>);
impl CredentialMaterial {
    pub fn bearer(value: String) -> Result<Self, Error> {
        let value = Zeroizing::new(value);
        if value.is_empty()
            || value.len() > MAX_SECRET
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
        {
            return Err(Error::InvalidCredential);
        }
        Ok(Self(value))
    }
}
impl fmt::Debug for CredentialMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialMaterial([omitted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    workspace: WorkspaceId,
    server: String,
    reference: String,
}
impl Key {
    fn of(profile: &RemoteProfile) -> Result<Self, Error> {
        Ok(Self {
            workspace: profile.config.workspace.clone(),
            server: profile.config.server.clone(),
            reference: profile
                .config
                .credential_ref
                .clone()
                .ok_or(Error::MissingCredential)?,
        })
    }
}
struct Entry {
    revision: Revision,
    profile: String,
    authority: AuthorityPin,
    expires_at: Timestamp,
    material: Option<Arc<CredentialMaterial>>,
}
#[derive(Default)]
struct State {
    entries: BTreeMap<Key, Entry>,
    closed: bool,
}

/// Owner-scoped resolver. Retain one instance per canonical owner. Revocation and
/// transport checks share this mutex, so no write may pass an already-committed
/// revocation. When combined with lifecycle admission use lifecycle -> resolver
/// lock order; never invoke the canonical worker from inside with_current.
#[derive(Default)]
pub struct CredentialResolver {
    state: Arc<Mutex<State>>,
}
impl CredentialResolver {
    pub fn install(
        &self,
        profile: &RemoteProfile,
        authority: AuthorityPin,
        expected: Option<Revision>,
        material: CredentialMaterial,
        expires_at: Timestamp,
        now: Timestamp,
    ) -> Result<Revision, Error> {
        if expires_at <= now {
            return Err(Error::Expired);
        }
        let key = Key::of(profile)?;
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        if state.closed {
            return Err(Error::Revoked);
        }
        let revision = match state.entries.get(&key) {
            Some(entry) if Some(entry.revision) == expected => {
                entry.revision.next().map_err(|_| Error::Unavailable)?
            }
            None if expected.is_none() && state.entries.len() < MAX_CREDENTIALS => Revision::ZERO,
            _ => return Err(Error::Stale),
        };
        state.entries.insert(
            key,
            Entry {
                revision,
                profile: profile.digest.clone(),
                authority,
                expires_at,
                material: Some(Arc::new(material)),
            },
        );
        Ok(revision)
    }
    pub fn resolve(
        &self,
        profile: &RemoteProfile,
        authority: &AuthorityPin,
        expected: Revision,
        now: Timestamp,
    ) -> Result<CredentialLease, Error> {
        let key = Key::of(profile)?;
        let state = self.state.lock().map_err(|_| Error::Unavailable)?;
        if state.closed {
            return Err(Error::Revoked);
        }
        let entry = state.entries.get(&key).ok_or(Error::MissingCredential)?;
        validate(entry, &profile.digest, authority, expected, now)?;
        Ok(CredentialLease {
            state: self.state.clone(),
            key,
            revision: expected,
            profile: profile.digest.clone(),
            authority: authority.clone(),
            expires_at: entry.expires_at,
            material: entry.material.clone().ok_or(Error::Revoked)?,
        })
    }
    pub fn revoke(&self, profile: &RemoteProfile, expected: Revision) -> Result<Revision, Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        let entry = state
            .entries
            .get_mut(&Key::of(profile)?)
            .ok_or(Error::MissingCredential)?;
        if entry.revision != expected || entry.profile != profile.digest {
            return Err(Error::Stale);
        }
        entry.revision = entry.revision.next().map_err(|_| Error::Unavailable)?;
        entry.material = None;
        Ok(entry.revision)
    }
    /// Owner shutdown invalidates every outstanding lease without reading secrets.
    pub fn close(&self) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        state.closed = true;
        for entry in state.entries.values_mut() {
            entry.material = None;
        }
        Ok(())
    }
}

/// Opaque lease: exact workspace/server/reference/profile/revision and owner pin.
/// Old material remains available only for sanitizing late replies after revoke.
#[derive(Clone)]
pub struct CredentialLease {
    state: Arc<Mutex<State>>,
    key: Key,
    revision: Revision,
    profile: String,
    authority: AuthorityPin,
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
    /// Recheck a newly observed canonical pin before each request is admitted.
    pub fn validate_authority(
        &self,
        profile: &RemoteProfile,
        authority: &AuthorityPin,
        now: Timestamp,
    ) -> Result<(), Error> {
        if profile.digest != self.profile
            || authority != &self.authority
            || Key::of(profile)? != self.key
        {
            return Err(Error::Stale);
        }
        self.with_current(now, || ())
    }
    /// Transport-only closure; hold through the actual underlying socket poll.
    pub(crate) fn with_current<T>(
        &self,
        now: Timestamp,
        action: impl FnOnce() -> T,
    ) -> Result<T, Error> {
        let state = self.state.lock().map_err(|_| Error::Unavailable)?;
        if state.closed {
            return Err(Error::Revoked);
        }
        let entry = state
            .entries
            .get(&self.key)
            .ok_or(Error::MissingCredential)?;
        validate(entry, &self.profile, &self.authority, self.revision, now)?;
        Ok(action())
    }
    /// Exact-byte removal for headers echoed in permitted content/errors. Parse
    /// untrusted metadata before capture only in private adapter memory; reject
    /// credential-bearing identities rather than changing their schema identity.
    /// Encoded/transformed secrets are not claimed to be detected by this filter.
    pub fn sanitize(&self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        if bytes.len() > MAX_CAPTURE {
            return Err(Error::CaptureLimit);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidContent)?;
        let marker = if "[credential omitted]".contains(self.material.0.as_str()) {
            ""
        } else {
            "[credential omitted]"
        };
        let mut output = Vec::with_capacity(bytes.len());
        let mut previous = 0;
        for (index, matched) in text.match_indices(self.material.0.as_str()) {
            if output
                .len()
                .saturating_add(index - previous)
                .saturating_add(marker.len())
                > MAX_CAPTURE
            {
                return Err(Error::CaptureLimit);
            }
            output.extend_from_slice(&bytes[previous..index]);
            output.extend_from_slice(marker.as_bytes());
            previous = index + matched.len();
        }
        if output.len().saturating_add(bytes.len() - previous) > MAX_CAPTURE {
            return Err(Error::CaptureLimit);
        }
        output.extend_from_slice(&bytes[previous..]);
        if self.contains_secret(&output)? {
            return Err(Error::InvalidContent);
        }
        Ok(output)
    }
    /// Normalize a complete bounded JSON frame privately before redaction, so
    /// ordinary JSON escapes cannot hide an echoed credential. Never persist the
    /// original frame. This is capture normalization, not protocol validation:
    /// validate the original bytes privately first (including duplicate keys).
    /// Credential-bearing object keys are rejected rather than
    /// renamed; admitted schema/tool identities must separately be rejected.
    pub fn sanitize_json(&self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        if bytes.len() > MAX_JSON_FRAME {
            return Err(Error::CaptureLimit);
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| Error::InvalidContent)?;
        fn visit(
            lease: &CredentialLease,
            value: &mut serde_json::Value,
            depth: usize,
        ) -> Result<(), Error> {
            if depth > 128 {
                return Err(Error::CaptureLimit);
            }
            match value {
                serde_json::Value::String(text) => {
                    *text = String::from_utf8(lease.sanitize(text.as_bytes())?)
                        .map_err(|_| Error::InvalidContent)?
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        visit(lease, value, depth + 1)?;
                    }
                }
                serde_json::Value::Object(values) => {
                    for (key, value) in values {
                        if lease.contains_secret(key.as_bytes())? {
                            return Err(Error::InvalidContent);
                        }
                        visit(lease, value, depth + 1)?;
                    }
                }
                scalar => {
                    if lease.contains_secret(scalar.to_string().as_bytes())? {
                        return Err(Error::InvalidContent);
                    }
                }
            }
            Ok(())
        }
        visit(self, &mut value, 0)?;
        struct Bounded(Vec<u8>);
        impl std::io::Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > MAX_CAPTURE {
                    return Err(std::io::Error::other("capture limit"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut output = Bounded(Vec::new());
        serde_json::to_writer(&mut output, &value).map_err(|_| Error::CaptureLimit)?;
        if self.contains_secret(&output.0)? {
            return Err(Error::InvalidContent);
        }
        Ok(output.0)
    }
    pub fn contains_secret(&self, bytes: &[u8]) -> Result<bool, Error> {
        if bytes.len() > MAX_CAPTURE {
            return Err(Error::CaptureLimit);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidContent)?;
        Ok(text.contains(self.material.0.as_str()))
    }
    /// Only an admitted transport may construct its ephemeral Authorization header.
    pub(crate) fn with_bearer<T>(
        &self,
        now: Timestamp,
        action: impl FnOnce(&str) -> T,
    ) -> Result<T, Error> {
        self.with_current(now, || action(self.material.0.as_str()))
    }
}
fn validate(
    entry: &Entry,
    profile: &str,
    authority: &AuthorityPin,
    revision: Revision,
    now: Timestamp,
) -> Result<(), Error> {
    if entry.revision != revision || entry.profile != profile || &entry.authority != authority {
        return Err(Error::Stale);
    }
    if entry.material.is_none() {
        return Err(Error::Revoked);
    }
    if entry.expires_at <= now {
        return Err(Error::Expired);
    }
    Ok(())
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidProfile,
    InvalidCredential,
    MissingCredential,
    Stale,
    Expired,
    Revoked,
    Unavailable,
    CaptureLimit,
    InvalidContent,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidProfile => "remote profile rejected",
            Self::InvalidCredential => "credential format rejected",
            Self::MissingCredential => "scoped credential unavailable",
            Self::Stale => "credential binding changed",
            Self::Expired => "credential expired",
            Self::Revoked => "credential revoked",
            Self::Unavailable => "credential resolver unavailable",
            Self::CaptureLimit => "sanitized capture limit exceeded",
            Self::InvalidContent => "credential-bearing capture rejected",
        })
    }
}
impl std::error::Error for Error {}

impl Drop for CredentialResolver {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> RemoteProfile {
        RemoteProfile::new(RemoteProfileConfig {
            workspace: WorkspaceId::new(),
            server: "fixture".into(),
            revision: Revision::ZERO,
            endpoint: "https://example.test/mcp".into(),
            credential_ref: Some("fixture-token".into()),
        })
        .unwrap()
    }
    fn pin() -> AuthorityPin {
        AuthorityPin {
            controller: ControllerId::new(),
            owner: OwnerEpoch::new(1),
            authority: AuthorityRevision::ZERO,
            binding: Revision::ZERO,
        }
    }
    fn install(
        resolver: &CredentialResolver,
        profile: &RemoteProfile,
        pin: &AuthorityPin,
    ) -> CredentialLease {
        let revision = resolver
            .install(
                profile,
                pin.clone(),
                None,
                CredentialMaterial::bearer("synthetic-credential".into()).unwrap(),
                Timestamp::new(100),
                Timestamp::ZERO,
            )
            .unwrap();
        resolver
            .resolve(profile, pin, revision, Timestamp::ZERO)
            .unwrap()
    }
    #[test]
    fn exact_https_profiles_reject_auth_queries_and_ambiguous_urls() {
        let p = profile();
        for endpoint in [
            "http://example.test/mcp",
            "https://user@example.test/mcp",
            "https://:password@example.test/mcp",
            "https://example.test/mcp?token=secret",
            "https://example.test/mcp?",
            "https://example.test/mcp#",
            "https://example.test/a/../mcp",
            "https://EXAMPLE.test/mcp",
            "https://example.test\\mcp",
            " https://example.test/mcp",
        ] {
            let mut config = p.config.clone();
            config.endpoint = endpoint.into();
            assert!(RemoteProfile::new(config).is_err(), "{endpoint}");
        }
        assert_eq!(
            RemoteProfile::new(p.config.clone()).unwrap().digest(),
            p.digest()
        );
        let mut changed = p.config.clone();
        changed.endpoint = "https://example.test/other".into();
        assert_ne!(RemoteProfile::new(changed).unwrap().digest(), p.digest());
        assert!(!serde_json::to_string(p.config())
            .unwrap()
            .contains("synthetic-credential"));
    }
    #[test]
    fn credentials_are_exactly_scoped_and_expire_without_grants() {
        let resolver = CredentialResolver::default();
        let p = profile();
        let authority = pin();
        let lease = install(&resolver, &p, &authority);
        assert!(lease
            .validate_authority(&p, &authority, Timestamp::new(99))
            .is_ok());
        assert_eq!(
            lease.with_current(Timestamp::new(100), || ()),
            Err(Error::Expired)
        );
        let mut changed = authority.clone();
        changed.owner = OwnerEpoch::new(2);
        assert_eq!(
            lease.validate_authority(&p, &changed, Timestamp::ZERO),
            Err(Error::Stale)
        );
        changed = authority.clone();
        changed.authority = AuthorityRevision::new(1);
        assert_eq!(
            lease.validate_authority(&p, &changed, Timestamp::ZERO),
            Err(Error::Stale)
        );
        for field in 0..4 {
            let mut config = p.config.clone();
            match field {
                0 => config.workspace = WorkspaceId::new(),
                1 => config.server = "other".into(),
                2 => config.credential_ref = Some("other".into()),
                _ => config.endpoint = "https://other.test/mcp".into(),
            }
            let other = RemoteProfile::new(config).unwrap();
            assert!(resolver
                .resolve(&other, &authority, Revision::ZERO, Timestamp::ZERO)
                .is_err());
            assert!(lease
                .validate_authority(&other, &authority, Timestamp::ZERO)
                .is_err());
        }
        assert_eq!(
            lease.with_bearer(Timestamp::ZERO, |value| value == "synthetic-credential"),
            Ok(true)
        );
    }
    #[test]
    fn rotation_revocation_and_owner_drop_invalidate_old_leases() {
        let resolver = CredentialResolver::default();
        let p = profile();
        let authority = pin();
        let old = install(&resolver, &p, &authority);
        let revision = resolver
            .install(
                &p,
                authority.clone(),
                Some(Revision::ZERO),
                CredentialMaterial::bearer("rotated-fixture".into()).unwrap(),
                Timestamp::new(200),
                Timestamp::ZERO,
            )
            .unwrap();
        assert_eq!(old.with_current(Timestamp::ZERO, || ()), Err(Error::Stale));
        let current = resolver
            .resolve(&p, &authority, revision, Timestamp::ZERO)
            .unwrap();
        assert!(resolver.revoke(&p, Revision::ZERO).is_err());
        resolver.revoke(&p, revision).unwrap();
        assert!(current.with_current(Timestamp::ZERO, || ()).is_err());
        assert_eq!(
            old.sanitize(b"late synthetic-credential").unwrap(),
            b"late [credential omitted]"
        );
        drop(resolver);
        assert_eq!(
            old.with_current(Timestamp::ZERO, || ()),
            Err(Error::Revoked)
        );
    }
    #[test]
    fn revocation_linearizes_after_in_progress_write_and_before_next_write() {
        let resolver = Arc::new(CredentialResolver::default());
        let p = profile();
        let authority = pin();
        let lease = install(&resolver, &p, &authority);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            lease.with_current(Timestamp::ZERO, || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        started_rx.recv().unwrap();
        assert!(
            resolver.state.try_lock().is_err(),
            "write must hold the revocation fence"
        );
        let pending = resolver.clone();
        let remote = p.clone();
        let revoker = std::thread::spawn(move || pending.revoke(&remote, Revision::ZERO));
        release_tx.send(()).unwrap();
        assert!(writer.join().unwrap().is_ok());
        let revision = revoker.join().unwrap().unwrap();
        assert!(resolver
            .resolve(&p, &authority, revision, Timestamp::ZERO)
            .is_err());
    }
    #[test]
    fn secret_capture_and_errors_are_bounded_and_redacted() {
        let resolver = CredentialResolver::default();
        let lease = install(&resolver, &profile(), &pin());
        assert_eq!(format!("{lease:?}"), "CredentialLease([omitted])");
        let text = b"Authorization: Bearer synthetic-credential; error synthetic-credential";
        let sanitized = lease.sanitize(text).unwrap();
        assert!(!lease.contains_secret(&sanitized).unwrap());
        assert!(lease.contains_secret(text).unwrap());
        assert_eq!(lease.sanitize(&[255]), Err(Error::InvalidContent));
        assert_eq!(
            lease.sanitize(&vec![b'x'; MAX_CAPTURE + 1]),
            Err(Error::CaptureLimit)
        );
        for value in ["", "bad\r\nheader", "space token", "not:bearer"] {
            assert!(CredentialMaterial::bearer(value.into()).is_err());
        }
        assert_eq!(
            format!(
                "{:?}",
                CredentialMaterial::bearer("synthetic-credential".into()).unwrap()
            ),
            "CredentialMaterial([omitted])"
        );
    }
    #[test]
    fn complete_json_escape_normalization_precedes_secret_capture() {
        let resolver = CredentialResolver::default();
        let lease = install(&resolver, &profile(), &pin());
        let escaped =
            br#"{"content":[{"text":"\u0073ynthetic-credential"}],"error":"synthetic-credential"}"#;
        let clean = lease.sanitize_json(escaped).unwrap();
        assert!(!lease.contains_secret(&clean).unwrap());
        let value: serde_json::Value = serde_json::from_slice(&clean).unwrap();
        assert_eq!(value["content"][0]["text"], "[credential omitted]");
        assert_eq!(value["error"], "[credential omitted]");
        assert!(lease
            .sanitize_json(br#"{"\u0073ynthetic-credential":true}"#)
            .is_err());
        assert!(lease.sanitize_json(br#"{"text":"partial"#).is_err());
        assert_eq!(
            lease.sanitize_json(&vec![b' '; MAX_JSON_FRAME + 1]),
            Err(Error::CaptureLimit)
        );
    }
    #[test]
    fn short_secrets_cannot_survive_inside_replacement_or_json_scalars() {
        for secret in ["credential", "omitted", "123", "true", "null"] {
            let resolver = CredentialResolver::default();
            let p = profile();
            let authority = pin();
            let revision = resolver
                .install(
                    &p,
                    authority.clone(),
                    None,
                    CredentialMaterial::bearer(secret.into()).unwrap(),
                    Timestamp::new(100),
                    Timestamp::ZERO,
                )
                .unwrap();
            let lease = resolver
                .resolve(&p, &authority, revision, Timestamp::ZERO)
                .unwrap();
            assert!(!lease
                .contains_secret(&lease.sanitize(secret.as_bytes()).unwrap())
                .unwrap());
            if ["123", "true", "null"].contains(&secret) {
                assert!(lease.sanitize_json(secret.as_bytes()).is_err());
            }
        }
    }
}
