// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{Entry, OwnerLease};
use std::future::Future;
use std::sync::Weak;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn fixture() -> (Lifecycle, OwnerLease, ThreadId) {
    let (runtime, owner) = Lifecycle::new(Duration::from_secs(2));
    let thread = ThreadId::new();
    {
        let mut state = runtime.0.state.lock().unwrap();
        state.root = Some(thread);
        state.entries.insert(
            thread,
            Entry {
                thread: Weak::new(),
                parent: None,
                held: false,
                interrupted: false,
                interruption_error: None,
                starts: 0,
                admission_generation: 0,
            },
        );
    }
    (runtime, owner, thread)
}
fn outbound(address: SocketAddr) -> Outbound {
    Outbound {
        address,
        authority: address.to_string(),
        path: "/mcp".into(),
        tls: None,
        headers: HeaderMap::new(),
        body: b"{}".to_vec(),
        limits: Limits {
            request_bytes: 4096,
            response_bytes: 4096,
            wire_bytes: 64 * 1024,
            header_bytes: 8192,
            header_count: 16,
            deadline: Instant::now() + Duration::from_secs(5),
        },
    }
}
async fn blocked(control: &SocketControl) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if control.0.lock().unwrap().blocked {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn revoke(runtime: &Lifecycle) {
    drop(runtime.hold_owner().unwrap());
}

#[tokio::test]
async fn credential_revocation_alone_prevents_the_next_physical_write() {
    use crate::foundation::mcp::remote_authority::*;
    use vcp_domain::{ids::*, revision::*};
    let (runtime, _owner, thread) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let profile = RemoteProfile::new(RemoteProfileConfig {
        workspace: WorkspaceId::new(),
        server: "fixture".into(),
        revision: Revision::ZERO,
        endpoint: "https://example.test/mcp".into(),
        credential_ref: Some("fixture-token".into()),
    })
    .unwrap();
    let pin = AuthorityPin {
        controller: ControllerId::new(),
        owner: OwnerEpoch::new(1),
        authority: AuthorityRevision::ZERO,
        binding: Revision::ZERO,
    };
    let resolver = CredentialResolver::default();
    let instant = now();
    let revision = resolver
        .install(
            &profile,
            pin.clone(),
            None,
            CredentialMaterial::bearer("synthetic-native-credential".into()).unwrap(),
            Timestamp::new(instant.get() + 10_000),
            instant,
        )
        .unwrap();
    let lease = resolver.resolve(&profile, &pin, revision, instant).unwrap();
    let control = Arc::new(SocketControl::new());
    control.0.lock().unwrap().allowance = Some(0);
    let attempt = tokio::spawn(exchange_control(
        runtime.clone(),
        thread,
        0,
        outbound(listener.local_addr().unwrap()),
        Some(lease),
        control.clone(),
    ));
    let (mut peer, _) = listener.accept().await.unwrap();
    blocked(&control).await;
    resolver.revoke(&profile, revision).unwrap();
    assert!(!runtime.0.state.lock().unwrap().held(thread));
    // Credential rotation has no lifecycle hold: wake the pending physical
    // write independently, proving that its own lease branch rejects it.
    let wake = control.0.lock().unwrap().waker.take();
    if let Some(wake) = wake {
        wake.wake();
    }
    let failure = match attempt.await.unwrap() {
        Err(error) => error,
        Ok(_) => panic!("revoked credential sent"),
    };
    assert_eq!(failure.kind, FailureKind::Revoked);
    assert_eq!(failure.sent_bytes, 0);
    let mut byte = [0];
    assert_eq!(peer.read(&mut byte).await.unwrap(), 0);
}

#[tokio::test]
async fn stale_generation_fails_before_connect_even_when_scope_is_running() {
    let (runtime, _owner, thread) = fixture();
    runtime
        .0
        .state
        .lock()
        .unwrap()
        .entries
        .get_mut(&thread)
        .unwrap()
        .admission_generation = 1;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let failure = match exchange(
        runtime.clone(),
        thread,
        0,
        outbound(listener.local_addr().unwrap()),
        None,
    )
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("stale generation connected"),
    };
    assert_eq!(failure.kind, FailureKind::Revoked);
    assert_eq!(failure.sent_bytes, 0);
    assert!(!failure.request_submitted);
    assert!(!runtime.0.state.lock().unwrap().held(thread));
    assert!(
        tokio::time::timeout(Duration::from_millis(40), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn physical_write_gate_revokes_zero_and_partial_requests_without_replay() {
    for prefix in [0, 4] {
        let (runtime, _owner, thread) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let control = Arc::new(SocketControl::new());
        control.0.lock().unwrap().allowance = Some(prefix);
        let attempt = tokio::spawn(exchange_control(
            runtime.clone(),
            thread,
            0,
            outbound(address),
            None,
            control.clone(),
        ));
        let (mut peer, _) = listener.accept().await.unwrap();
        blocked(&control).await;
        revoke(&runtime);
        let failure = match attempt.await.unwrap() {
            Err(failure) => failure,
            Ok(_) => panic!("revoked request succeeded"),
        };
        assert_eq!(failure.kind, FailureKind::Revoked);
        assert_eq!(failure.sent_bytes, prefix as u64);
        assert!(
            failure.request_submitted,
            "partial or enqueued writes are never inferred absent"
        );
        let mut received = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), peer.read_to_end(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.len(), prefix);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), listener.accept())
                .await
                .is_err(),
            "no reconnect or replay"
        );
    }
}

#[tokio::test]
async fn owner_loss_closes_socket_even_when_caller_stops_polling() {
    for detached in [false, true] {
        let (runtime, owner, thread) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let control = Arc::new(SocketControl::new());
        control.0.lock().unwrap().allowance = Some(0);
        let attempt = exchange_control(
            runtime.clone(),
            thread,
            0,
            outbound(listener.local_addr().unwrap()),
            None,
            control.clone(),
        );
        tokio::pin!(attempt);
        // Manually poll to the raw-write barrier, then never poll this future while
        // owner loss must independently close the socket.
        tokio::time::timeout(
            Duration::from_secs(2),
            poll_fn(|cx| {
                let _ = attempt.as_mut().poll(cx);
                if control.0.lock().unwrap().blocked {
                    Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }),
        )
        .await
        .unwrap();
        let (mut peer, _) = listener.accept().await.unwrap();
        if detached {
            runtime.0.state.lock().unwrap().attached = false;
        }
        drop(owner);
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), peer.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(control.0.lock().unwrap().socket.is_none());
    }
}

async fn response_case(
    response: &'static [u8],
    mut configure: impl FnMut(&mut Outbound),
) -> Result<Response, Failure> {
    let (runtime, _owner, thread) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut request = outbound(listener.local_addr().unwrap());
    configure(&mut request);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut byte = [0];
        while !bytes.ends_with(b"\r\n\r\n{}") {
            if socket.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            bytes.push(byte[0]);
            assert!(bytes.len() < 8192);
        }
        assert!(bytes.starts_with(b"POST /mcp HTTP/1.1\r\n"));
        let _ = socket.write_all(response).await;
        bytes
    });
    let result = exchange(runtime, thread, 0, request, None).await;
    server.await.unwrap();
    result
}

#[tokio::test]
async fn bounded_response_rejects_encoding_oversize_and_retains_redirect_without_following() {
    let response = response_case(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}",
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(response.body, b"{}");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers[header::CONTENT_TYPE], "application/json");
    assert!(response.sent_bytes > 2 && response.received_bytes > 2);
    for bytes in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 999999\r\n\r\n".as_slice(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Encoding: gzip\r\n\r\n{}".as_slice(),
    ] {
        assert!(response_case(bytes, |_| {}).await.is_err());
    }
    let response = response_case(
        b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 0\r\n\r\n",
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(response.status, StatusCode::FOUND);
}

#[tokio::test]
async fn absolute_deadline_and_drop_close_owned_driver() {
    for cancel in [false, true] {
        let (runtime, _owner, thread) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut request = outbound(listener.local_addr().unwrap());
        request.limits.deadline = Instant::now() + Duration::from_millis(150);
        let attempt = tokio::spawn(exchange(runtime, thread, 0, request, None));
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 8192];
        assert!(peer.read(&mut bytes).await.unwrap() > 0);
        if cancel {
            attempt.abort();
            match attempt.await {
                Err(error) => assert!(error.is_cancelled()),
                Ok(_) => panic!("cancelled request completed"),
            };
        } else {
            let failure = match attempt.await.unwrap() {
                Err(error) => error,
                Ok(_) => panic!("silent server succeeded"),
            };
            assert_eq!(failure.kind, FailureKind::Deadline);
            assert!(failure.request_submitted);
        }
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), peer.read(&mut bytes))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}

fn tls_config() -> (Arc<ClientConfig>, Arc<rustls::ServerConfig>) {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let client = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        )
        .unwrap();
    (Arc::new(client), Arc::new(server))
}

#[tokio::test]
async fn https_requires_exact_trusted_server_identity_before_http_bytes() {
    for valid_name in [true, false] {
        let (runtime, _owner, thread) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (client, server) = tls_config();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let Ok(mut tls) = tokio_rustls::TlsAcceptor::from(server).accept(socket).await else {
                return Vec::new();
            };
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n{}") {
                match tls.read(&mut byte).await {
                    Ok(0) | Err(_) => return bytes,
                    Ok(_) => bytes.push(byte[0]),
                }
                assert!(bytes.len() < 8192);
            }
            tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .unwrap();
            tls.flush().await.unwrap();
            let _ = tls.shutdown().await;
            bytes
        });
        let mut request = outbound(address);
        let name = if valid_name {
            "localhost"
        } else {
            "wrong.test"
        };
        request.authority = format!("{name}:{}", address.port());
        request.tls = Some(Tls {
            name: ServerName::try_from(name).unwrap(),
            config: client,
        });
        let response = exchange(runtime, thread, 0, request, None).await;
        let observed = tokio::time::timeout(Duration::from_secs(2), peer)
            .await
            .unwrap()
            .unwrap();
        if valid_name {
            assert_eq!(response.unwrap().body, b"{}");
            assert!(observed.starts_with(b"POST /mcp HTTP/1.1"));
        } else {
            let failure = match response {
                Err(error) => error,
                Ok(_) => panic!("wrong TLS name accepted"),
            };
            assert!(!failure.request_submitted);
            assert!(observed.is_empty());
        }
    }
}

#[tokio::test]
async fn buffered_tls_plaintext_cannot_flush_read_or_shutdown_after_revocation() {
    for operation in ["flush", "read", "shutdown"] {
        let (runtime, _owner, thread) = fixture();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (client, server) = tls_config();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut tls = tokio_rustls::TlsAcceptor::from(server)
                .accept(socket)
                .await
                .unwrap();
            let mut bytes = Vec::new();
            let _ = tls.read_to_end(&mut bytes).await;
            bytes
        });
        // Use the same raw gate directly to force TLS to accept plaintext into
        // its buffer while the physical writer returns Pending.
        let control = Arc::new(SocketControl::new());
        control.0.lock().unwrap().socket = Some(TcpStream::connect(address).await.unwrap());
        runtime
            .0
            .remote_sockets
            .lock()
            .unwrap()
            .insert(thread, vec![Arc::downgrade(&control)]);
        let socket = FencedSocket {
            control: control.clone(),
            runtime: runtime.clone(),
            thread,
            generation: 0,
            credential: None,
            limit: 64 * 1024,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let mut tls = tokio_rustls::TlsConnector::from(client)
            .connect(ServerName::try_from("localhost").unwrap(), socket)
            .await
            .unwrap();
        let sent = control.0.lock().unwrap().sent;
        control.0.lock().unwrap().allowance = Some(0);
        tls.write_all(b"buffered-private-payload").await.unwrap();
        assert!(control.0.lock().unwrap().blocked);
        assert_eq!(control.0.lock().unwrap().sent, sent);
        revoke(&runtime);
        let result = match operation {
            "flush" => tls.flush().await,
            "read" => {
                let mut byte = [0];
                tls.read(&mut byte).await.map(|_| ())
            }
            "shutdown" => tls.shutdown().await,
            _ => unreachable!(),
        };
        assert!(result.is_err(), "{operation} bypassed revocation");
        assert_eq!(control.0.lock().unwrap().sent, sent);
        drop(tls);
        assert!(tokio::time::timeout(Duration::from_secs(2), peer)
            .await
            .unwrap()
            .unwrap()
            .is_empty());
    }
}
