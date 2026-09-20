// SPDX-License-Identifier: Apache-2.0
use super::super::tests::{fixture, outbound, tls_config};
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn request(io: &mut (impl AsyncRead + Unpin)) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = io.read_u8().await.unwrap();
        bytes.push(byte);
        assert!(bytes.len() < 8192);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    bytes
}

#[tokio::test]
async fn full_flush_observation_is_bound_to_one_exchange_and_exact_body() {
    let (runtime, _owner, thread) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut io, _) = listener.accept().await.unwrap();
        request(&mut io).await;
        let mut body = [0; 2];
        io.read_exact(&mut body).await.unwrap();
        assert_eq!(&body, b"{}");
        io.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            .await
            .unwrap();
    });
    let mut exchange = Exchange::start(runtime.clone(), thread, 0, outbound(address), None)
        .await
        .unwrap();
    let mut proof = None;
    let mut body = Vec::new();
    loop {
        match exchange.next().await.unwrap() {
            Event::Written(value) => {
                assert!(proof.is_none());
                assert!(exchange.owns(&value));
                assert_eq!(value.body_digest(), vcp_protocol::digest_bytes(b"{}"));
                proof = Some(value);
            }
            Event::Head { status, .. } => assert_eq!(status, StatusCode::OK),
            Event::Chunk(bytes) => body.extend_from_slice(&bytes),
            Event::End { .. } => break,
        }
    }
    assert_eq!(body, b"{}");
    let proof = proof.unwrap();
    let other_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let other = Exchange::start(
        runtime,
        thread,
        0,
        outbound(other_listener.local_addr().unwrap()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(other.digest, proof.body_digest());
    assert!(!other.owns(&proof));
    peer.await.unwrap();
}

#[tokio::test]
async fn early_final_headers_do_not_certify_a_blocked_body() {
    let (runtime, _owner, thread) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut io, _) = listener.accept().await.unwrap();
        request(&mut io).await;
        io.write_all(b"HTTP/1.1 413 Content Too Large\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
        let mut rest = Vec::new();
        io.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
    });
    let mut exchange = Exchange::start(runtime, thread, 0, outbound(address), None)
        .await
        .unwrap();
    exchange.write.lock().unwrap().body_paused = true;
    let mut head = false;
    loop {
        match exchange.next().await.unwrap() {
            Event::Written(_) => panic!("early response certified unsent body"),
            Event::Head { status, .. } => {
                assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
                head = true;
            }
            Event::Chunk(_) => panic!("empty response contained data"),
            Event::End { .. } => break,
        }
    }
    assert!(head);
    assert!(!exchange.write.lock().unwrap().handed_off);
    peer.await.unwrap();
}

#[tokio::test]
async fn tls_buffered_body_is_not_written_until_the_physical_flush() {
    let (runtime, _owner, thread) = fixture();
    let (trust, server) = tls_config();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (io, _) = listener.accept().await.unwrap();
        let mut io = tokio_rustls::TlsAcceptor::from(server)
            .accept(io)
            .await
            .unwrap();
        let mut bytes = Vec::new();
        let _ = io.read_to_end(&mut bytes).await;
        bytes
    });
    let mut out = outbound(address);
    out.authority = format!("localhost:{}", address.port());
    out.tls = Some(Tls {
        name: ServerName::try_from("localhost").unwrap(),
        trust,
    });
    let control = Arc::new(SocketControl::new());
    let mut exchange =
        Exchange::start_control(runtime.clone(), thread, 0, out, None, control.clone())
            .await
            .unwrap();
    control.0.lock().unwrap().allowance = Some(0);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), exchange.next())
            .await
            .is_err()
    );
    {
        let state = exchange.write.lock().unwrap();
        assert!(state.handed_off);
        assert!(!state.written);
    }
    drop(runtime.hold_owner().unwrap());
    assert!(exchange.next().await.is_err());
    assert!(peer.await.unwrap().is_empty());
}

#[tokio::test]
async fn complete_chunk_is_visible_before_a_later_driver_failure() {
    let (runtime, _owner, thread) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, wait) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut io, _) = listener.accept().await.unwrap();
        request(&mut io).await;
        let mut body = [0; 2];
        io.read_exact(&mut body).await.unwrap();
        io.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ngood\r\n")
            .await
            .unwrap();
        wait.await.unwrap();
        io.write_all(b"invalid-chunk\r\n").await.unwrap();
    });
    let mut exchange = Exchange::start(runtime, thread, 0, outbound(address), None)
        .await
        .unwrap();
    loop {
        match exchange.next().await.unwrap() {
            Event::Chunk(bytes) => {
                assert_eq!(bytes.as_ref(), b"good");
                break;
            }
            Event::End { .. } => panic!("premature end"),
            _ => (),
        }
    }
    release.send(()).unwrap();
    assert!(exchange.next().await.is_err());
    peer.await.unwrap();
}
