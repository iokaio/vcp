// SPDX-License-Identifier: Apache-2.0
//! Packaged startup diagnostics, not an HTTP401 or interactive /mcp claim.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the exact extracted package; run explicit P8 qualification"]
async fn packaged_mcp_missing_or_invalid_credential_is_visible_without_network_or_provider_work() {
    std::env::var_os("VCP_TEST_SKILL_PACKAGE").expect("exact extracted package required");
    for backend in ["sqlite", "files"] {
        for invalid in [false, true] {
            let provider = MockServer::start().await;
            let mcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("https://127.0.0.1:{}/mcp", mcp.local_addr().unwrap().port());
            let mut f = Fixture::new(&provider.uri(), "complete");
            f.package(true);
            assert!(f
                .run(&["storage", "configure", "--backend", backend])
                .await
                .status
                .success());
            let variable = "VCP_P803_MCP_SYNTHETIC_CREDENTIAL";
            let canary = "P803 invalid synthetic bearer with spaces";
            let mut profile: Value =
                serde_json::from_slice(&fs::read(&f.profile).unwrap()).unwrap();
            profile["mcp_http"] = json!([{"name":"preflight-remote","endpoint":endpoint,
                "credential":{"reference":"preflight-auth","environment":variable},"allowed_tools":["echo"],
                "limits":{"frame_bytes":4096,"total_discovery_bytes":8192,"tools":8,"pages":2,"timeout_ms":10000,"stderr_bytes":0}}]);
            fs::write(&f.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
            let mut command = f.command(&[
                "run",
                "Observe configured MCP authentication",
                "--autonomy",
                "autonomous",
            ]);
            command.env_remove(variable);
            if invalid {
                command.env(variable, canary);
            }
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(90),
                tokio::task::spawn_blocking(move || command.output().unwrap()),
            )
            .await
            .unwrap()
            .unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            let public_diagnostic = diagnostic.replace(canary, "[synthetic credential omitted]");
            assert_eq!(
                output.status.code(),
                Some(2),
                "{backend}, invalid={invalid}: {public_diagnostic}"
            );
            assert!(
                diagnostic.contains(if invalid {
                    "configured MCP credential format rejected"
                } else {
                    "configured MCP credential variable is unavailable"
                }),
                "{backend}, invalid={invalid}: {public_diagnostic}"
            );
            assert!(!diagnostic.contains(canary));
            assert!(!String::from_utf8_lossy(&output.stdout).contains(canary));
            let rows = records(&output);
            assert!(
                rows.iter().all(|row| row["type"] != "accepted"),
                "credential setup must fail before durable task acceptance"
            );
            assert!(rows.last().unwrap()["scope"].is_null());
            assert_eq!(
                rows.last().unwrap()["conditions"]["invalid_configuration"],
                true
            );
            assert_eq!(rows.last().unwrap()["exit_code"], 2);
            assert_eq!(
                rows.last().unwrap()["conditions"]["internal_failure"],
                false
            );
            assert!(rows.last().unwrap()["receipt"].is_null());
            assert!(
                provider.received_requests().await.unwrap().is_empty(),
                "authentication setup must precede provider submission"
            );
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(150), mcp.accept())
                    .await
                    .is_err(),
                "missing or invalid credentials cannot open an MCP transport connection"
            );
        }
    }
}
