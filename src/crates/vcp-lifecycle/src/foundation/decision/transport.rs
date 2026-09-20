// SPDX-License-Identifier: Apache-2.0
//! Closed evaluator POSTs over the existing owned, physically fenced HTTP driver.
use super::credentials::CredentialLease;
use crate::{
    remote_transport::{self as wire, trust::TrustSnapshot},
    Lifecycle,
};
use codex_protocol::ThreadId;
use http::{header, HeaderMap, HeaderValue};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;
use vcp_domain::revision::Timestamp;
use vcp_models::decision::Operation;

#[derive(Clone)]
pub(crate) struct Destination {
    #[cfg(feature = "qualification")]
    after_tls: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    host: String,
    authority: String,
    port: u16,
    trust: Arc<TrustSnapshot>,
    fixture: bool,
}
impl Destination {
    pub(crate) fn production() -> Result<Self, String> {
        Ok(Self {
            #[cfg(feature = "qualification")]
            after_tls: None,
            host: "openrouter.ai".into(),
            authority: "openrouter.ai".into(),
            port: 443,
            trust: Arc::new(TrustSnapshot::native()?),
            fixture: false,
        })
    }
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) fn fixture(origin: String, roots: Vec<Vec<u8>>) -> Result<Self, String> {
        let url = url::Url::parse(&origin).map_err(|_| "invalid evaluator fixture origin")?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.as_str() != origin
        {
            return Err("invalid evaluator fixture origin".into());
        }
        let host = match url.host() {
            Some(url::Host::Domain("localhost")) => "localhost".into(),
            Some(url::Host::Ipv4(ip)) if ip.is_loopback() => ip.to_string(),
            Some(url::Host::Ipv6(ip)) if ip.is_loopback() => ip.to_string(),
            _ => return Err("evaluator fixture must be loopback".into()),
        };
        Ok(Self {
            #[cfg(feature = "qualification")]
            after_tls: None,
            host,
            authority: url[url::Position::BeforeHost..url::Position::AfterPort].into(),
            port: url.port_or_known_default().ok_or("invalid fixture port")?,
            trust: Arc::new(TrustSnapshot::fixture_roots(roots)?),
            fixture: true,
        })
    }
    pub(crate) fn fixture_origin(&self) -> bool {
        self.fixture
    }
    #[cfg(feature = "qualification")]
    pub(crate) fn with_after_tls(
        mut self,
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Self {
        self.after_tls = Some((arrived, release));
        self
    }
    pub(crate) fn trust_digest(&self) -> &str {
        self.trust.digest()
    }
}
pub(crate) struct Request {
    pub operation: Operation,
    pub body: Vec<u8>,
    pub body_digest: String,
    pub deadline: Instant,
}
pub(crate) struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub request_written: bool,
}
pub(crate) struct Failure {
    pub request_submitted: bool,
    pub reason: &'static str,
    /// Private bounded observation retained for accounting, never trusted advice.
    pub status: Option<u16>,
    pub body: Vec<u8>,
    pub request_written: bool,
}
impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failure")
            .field("reason", &self.reason)
            .field("request_submitted", &self.request_submitted)
            .finish_non_exhaustive()
    }
}
fn now() -> Timestamp {
    Timestamp::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    )
}

/// Host must durably admit the exact request and record SendIntent first.
/// The callback checks current source/authority before DNS, after DNS, and after
/// TLS before polling HTTP. Every physical socket poll additionally holds the
/// lifecycle and scoped-credential gates. No retry or alternate address attempt.
pub(crate) async fn send(
    runtime: Lifecycle,
    thread: ThreadId,
    generation: u64,
    request: Request,
    lease: CredentialLease,
    destination: Destination,
    mut revalidate: impl FnMut() -> Result<(), String> + Send,
) -> Result<Response, Failure> {
    let fail = |submitted, reason| Failure {
        request_submitted: submitted,
        reason,
        status: None,
        body: vec![],
        request_written: false,
    };
    if request.body.is_empty()
        || request.deadline <= Instant::now()
        || request.deadline.saturating_duration_since(Instant::now())
            > std::time::Duration::from_secs(120)
        || request.body.len() > 64 * 1024
        || vcp_protocol::digest_bytes(&request.body) != request.body_digest
        || lease.pin().operation != request.operation
        || lease.pin().fixture != destination.fixture
    {
        return Err(fail(false, "invalid evaluator dispatch"));
    }
    if runtime.admission_generation(thread).ok() != Some(generation) {
        return Err(fail(false, "evaluator owner admission changed"));
    }
    lease
        .with_current(now(), || ())
        .map_err(|_| fail(false, "evaluator credential unavailable"))?;
    revalidate().map_err(|_| fail(false, "evaluator source unavailable"))?;
    let addresses = tokio::time::timeout_at(
        request.deadline,
        tokio::net::lookup_host((destination.host.as_str(), destination.port)),
    )
    .await
    .map_err(|_| fail(false, "evaluator DNS deadline"))?
    .map_err(|_| fail(false, "evaluator DNS unavailable"))?;
    let mut addresses = addresses.take(17).collect::<Vec<_>>();
    if addresses.len() > 16 {
        return Err(fail(false, "evaluator DNS result limit"));
    }
    addresses.sort_unstable();
    addresses.dedup();
    let address = addresses
        .into_iter()
        .next()
        .ok_or_else(|| fail(false, "evaluator recipient unavailable"))?;
    if destination.fixture && !address.ip().is_loopback() {
        return Err(fail(false, "invalid evaluator fixture address"));
    }
    revalidate().map_err(|_| fail(false, "evaluator source changed after DNS"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    let mut authorization = lease
        .with_bearer(now(), |secret| {
            let text = zeroize::Zeroizing::new(format!("Bearer {secret}"));
            HeaderValue::from_str(&text)
        })
        .map_err(|_| fail(false, "evaluator credential unavailable"))?
        .map_err(|_| fail(false, "invalid evaluator credential header"))?;
    authorization.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, authorization);
    let endpoint = url::Url::parse(request.operation.endpoint())
        .map_err(|_| fail(false, "invalid closed evaluator endpoint"))?;
    let out = wire::Outbound {
        address,
        authority: destination.authority,
        path: endpoint.path().into(),
        tls: Some(wire::Tls {
            name: rustls::pki_types::ServerName::try_from(destination.host)
                .map_err(|_| fail(false, "invalid evaluator TLS name"))?,
            trust: destination.trust,
        }),
        headers,
        body: request.body,
        limits: wire::Limits {
            request_bytes: 64 * 1024,
            response_bytes: 64 * 1024,
            wire_bytes: 2 * 1024 * 1024,
            header_bytes: 16 * 1024,
            header_count: 32,
            deadline: request.deadline,
        },
    };
    let mut exchange = wire::Exchange::start(runtime, thread, generation, out, Some(lease.into()))
        .await
        .map_err(|error| fail(error.request_submitted, "evaluator connection unavailable"))?;
    #[cfg(feature = "qualification")]
    if let Some((arrived, release)) = destination.after_tls {
        arrived.notify_one();
        tokio::time::timeout_at(request.deadline, release.notified())
            .await
            .map_err(|_| fail(false, "evaluator qualification TLS barrier deadline"))?;
    }
    // start owns an UNPOLLED HTTP driver. This check precedes application bytes.
    revalidate().map_err(|_| fail(false, "evaluator source changed during TLS"))?;
    let mut status = None;
    let mut body = Vec::new();
    let mut written = false;
    loop {
        let event = match exchange.next().await {
            Ok(event) => event,
            Err(error) => {
                return Err(Failure {
                    request_submitted: error.request_submitted,
                    reason: "evaluator response unavailable",
                    status,
                    body,
                    request_written: written,
                })
            }
        };
        match event {
            wire::Event::Written(proof) => {
                if !exchange.owns(&proof) || proof.body_digest() != request.body_digest {
                    return Err(Failure {
                        status,
                        body,
                        request_written: written,
                        ..fail(true, "evaluator write identity changed")
                    });
                }
                written = true;
            }
            wire::Event::Head {
                status: code,
                headers,
            } => {
                if status.is_some() || headers.contains_key(header::CONTENT_ENCODING) {
                    return Err(Failure {
                        status: Some(code.as_u16()),
                        body,
                        request_written: written,
                        ..fail(true, "unsupported evaluator response")
                    });
                }
                if code.is_success() {
                    let mut types = headers.get_all(header::CONTENT_TYPE).iter();
                    let content = types.next().and_then(|value| value.to_str().ok());
                    if types.next().is_some()
                        || !matches!(
                            content,
                            Some("application/json" | "application/json; charset=utf-8")
                        )
                    {
                        return Err(Failure {
                            status: Some(code.as_u16()),
                            body,
                            request_written: written,
                            ..fail(true, "unsupported evaluator response media")
                        });
                    }
                }
                status = Some(code.as_u16());
            }
            wire::Event::Chunk(bytes) => {
                if body.len().saturating_add(bytes.len()) > 64 * 1024 {
                    return Err(Failure {
                        status,
                        body,
                        request_written: written,
                        ..fail(true, "evaluator response limit")
                    });
                }
                body.extend_from_slice(&bytes);
            }
            wire::Event::End { .. } => {
                return Ok(Response {
                    status: status.ok_or_else(|| fail(true, "evaluator response head missing"))?,
                    body,
                    request_written: written,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decision_fixture_recipient_cannot_select_remote_or_url_credentials() {
        for origin in [
            "https://example.com/",
            "http://localhost:443/",
            "https://user:secret@localhost/",
            "https://localhost/path",
            "https://localhost/?token=secret",
            "https://localhost/#secret",
        ] {
            assert!(Destination::fixture(origin.into(), vec![]).is_err());
        }
    }
    #[test]
    fn decision_failure_keeps_private_observation_without_debug_payload() {
        let failure = Failure {
            request_submitted: true,
            reason: "evaluator response unavailable",
            status: Some(200),
            body: b"sensitive-late-response".to_vec(),
            request_written: true,
        };
        assert_eq!(failure.body, b"sensitive-late-response");
        assert!(!format!("{failure:?}").contains("sensitive"));
    }
}
