// SPDX-License-Identifier: Apache-2.0
use super::*;

fn certificate(name: &str) -> Vec<u8> {
    rcgen::generate_simple_self_signed(vec![name.into()])
        .unwrap()
        .cert
        .der()
        .to_vec()
}

#[test]
fn trust_identity_is_order_independent_but_binds_distrust_and_roots() {
    let first = certificate("localhost");
    let second = certificate("other.test");
    let one = TrustSnapshot::build(vec![first.clone(), second.clone()], vec![]).unwrap();
    let reordered =
        TrustSnapshot::build(vec![second.clone(), first.clone(), first.clone()], vec![]).unwrap();
    assert_eq!(one.digest(), reordered.digest());
    let denied = TrustSnapshot::build(vec![first.clone(), second.clone()], vec![second]).unwrap();
    assert_ne!(one.digest(), denied.digest());
    assert!(TrustSnapshot::build(vec![first.clone()], vec![first]).is_err());
    assert!(!one.config().enable_early_data);
    assert_eq!(one.config().alpn_protocols, vec![b"http/1.1".to_vec()]);
}

#[test]
fn invalid_empty_or_excessive_snapshots_fail_closed() {
    assert!(TrustSnapshot::build(vec![], vec![]).is_err());
    assert!(TrustSnapshot::build(vec![b"not-a-certificate".to_vec()], vec![]).is_err());
    assert!(TrustSnapshot::build(vec![vec![0; MAX_CERTIFICATE_BYTES + 1]], vec![]).is_err());
    let mut budget = Budget::default();
    for _ in 0..MAX_CERTIFICATES {
        budget.observe_size(1).unwrap();
    }
    assert!(budget.observe_size(1).is_err());
    let mut budget = Budget::default();
    for _ in 0..MAX_TOTAL_BYTES / MAX_CERTIFICATE_BYTES {
        budget.observe_size(MAX_CERTIFICATE_BYTES).unwrap();
    }
    assert!(budget.observe_size(1).is_err());
}

#[test]
fn explicit_distrust_rejects_leaf_and_intermediate_without_weakening_normal_verifier() {
    let root = certificate("localhost");
    let other = certificate("other.test");
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(root.clone())).unwrap();
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
        .build()
        .unwrap();
    let verifier = Distrust {
        inner,
        denied: BTreeSet::from([vcp_protocol::digest_bytes(&other)]),
    };
    let root = CertificateDer::from(root);
    let other = CertificateDer::from(other);
    let name = ServerName::try_from("localhost").unwrap();
    assert!(verifier
        .verify_server_cert(&root, &[], &name, &[], UnixTime::now())
        .is_ok());
    assert!(verifier
        .verify_server_cert(
            &root,
            &[],
            &ServerName::try_from("wrong.test").unwrap(),
            &[],
            UnixTime::now()
        )
        .is_err());
    for (leaf, chain) in [(&other, vec![]), (&root, vec![other.clone()])] {
        assert!(matches!(
            verifier.verify_server_cert(leaf, &chain, &name, &[], UnixTime::now()),
            Err(rustls::Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure
            ))
        ));
    }
    assert!(!verifier.supported_verify_schemes().is_empty());
}

#[test]
fn native_snapshot_ignores_ambient_ca_paths_without_modifying_stores() {
    const CHILD: &str = "VCP_TRUST_SNAPSHOT_FIXTURE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let snapshot = TrustSnapshot::native().unwrap();
        assert_eq!(snapshot.digest().len(), 64);
        return;
    }
    // Set only the child environment; do not mutate global process variables or
    // read actual credentials. The sentinel paths deliberately do not exist.
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "remote_transport::trust::tests::native_snapshot_ignores_ambient_ca_paths_without_modifying_stores", "--nocapture"])
        .env(CHILD, "1")
        .env("SSL_CERT_FILE", "Z:/vcp-nonexistent-ca-fixture.pem")
        .env("SSL_CERT_DIR", "Z:/vcp-nonexistent-ca-fixture-directory")
        .output().unwrap();
    assert!(
        result.status.success(),
        "native trust child failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
}
