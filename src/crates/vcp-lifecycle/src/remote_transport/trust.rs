// SPDX-License-Identifier: Apache-2.0
//! Immutable local Windows trust snapshot. No environment overrides, chain
//! retrieval, private-key access, or OS store mutation. This is not the complete
//! Windows chain-policy/CTL/revocation engine.
use rustls::{
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        WebPkiServerVerifier,
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[path = "trust/windows.rs"]
mod windows;
const MAX_CERTIFICATES: usize = 8192;
const MAX_CERTIFICATE_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const POLICY: &str =
    "vcp-windows-root-snapshot/1;tls12+13;aws-lc;http1;no-resumption;exact-der-distrust";

#[derive(Clone)]
pub(crate) struct TrustSnapshot {
    config: Arc<ClientConfig>,
    digest: String,
}
impl std::fmt::Debug for TrustSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustSnapshot")
            .field("digest", &self.digest)
            .finish()
    }
}
impl TrustSnapshot {
    pub(crate) fn native() -> Result<Self, String> {
        let snapshot = windows::snapshot()?;
        Self::build(snapshot.roots, snapshot.denied)
    }
    pub(crate) fn config(&self) -> Arc<ClientConfig> {
        self.config.clone()
    }
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    #[cfg(any(test, feature = "qualification"))]
    pub(crate) fn fixture_roots(roots: Vec<Vec<u8>>) -> Result<Self, String> {
        Self::build(roots, Vec::new())
    }

    fn build(roots: Vec<Vec<u8>>, denied: Vec<Vec<u8>>) -> Result<Self, String> {
        let mut budget = Budget::default();
        let mut deny = BTreeSet::new();
        for der in denied {
            budget.observe(&der)?;
            deny.insert(vcp_protocol::digest_bytes(&der));
        }
        let mut unique = BTreeMap::new();
        for der in roots {
            budget.observe(&der)?;
            let hash = vcp_protocol::digest_bytes(&der);
            if !deny.contains(&hash) {
                unique.insert(hash, der);
            }
        }
        if unique.is_empty() {
            return Err("TLS trust snapshot has no usable roots".into());
        }
        let root_hashes: Vec<_> = unique.keys().cloned().collect();
        let mut store = RootCertStore::empty();
        for der in unique.into_values() {
            store
                .add(CertificateDer::from(der))
                .map_err(|_| "TLS root certificate rejected")?;
        }
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier =
            WebPkiServerVerifier::builder_with_provider(Arc::new(store), provider.clone())
                .build()
                .map_err(|_| "TLS root verifier rejected")?;
        // The wrapper only adds explicit distrust; all chain, name and handshake
        // signature checks are delegated unchanged to Rustls's normal verifier.
        let verifier = Arc::new(Distrust {
            inner: verifier,
            denied: deny.clone(),
        });
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| "TLS protocol configuration rejected")?
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        config.enable_early_data = false;
        config.resumption = rustls::client::Resumption::disabled();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let bytes = vcp_protocol::canonical_bytes(
            &serde_json::json!({"policy": POLICY, "roots": root_hashes, "denied": deny}),
        )
        .map_err(|_| "TLS trust identity could not be encoded")?;
        Ok(Self {
            config: Arc::new(config),
            digest: vcp_protocol::digest_bytes(&bytes),
        })
    }
}

#[derive(Default)]
struct Budget {
    count: usize,
    bytes: usize,
}
impl Budget {
    fn observe(&mut self, der: &[u8]) -> Result<(), String> {
        self.observe_size(der.len())
    }
    fn observe_size(&mut self, length: usize) -> Result<(), String> {
        if length == 0
            || length > MAX_CERTIFICATE_BYTES
            || self.count >= MAX_CERTIFICATES
            || length > MAX_TOTAL_BYTES.saturating_sub(self.bytes)
        {
            return Err("TLS trust snapshot exceeds certificate bounds".into());
        }
        self.count += 1;
        self.bytes += length;
        Ok(())
    }
}

#[derive(Debug)]
struct Distrust {
    inner: Arc<WebPkiServerVerifier>,
    denied: BTreeSet<String>,
}
impl ServerCertVerifier for Distrust {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if std::iter::once(end).chain(intermediates).any(|cert| {
            self.denied
                .contains(&vcp_protocol::digest_bytes(cert.as_ref()))
        }) {
            return Err(rustls::Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        self.inner
            .verify_server_cert(end, intermediates, name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, signature)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, signature)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[cfg(test)]
#[path = "trust/tests.rs"]
mod tests;
