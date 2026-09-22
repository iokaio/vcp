// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_child_adapter_accounts_once_and_verifies_integrated_parent() {
    for generation in [false, true] {
        let server = MockServer::start().await;
        let fixture = Fixture::new(&server.uri(), "complete");
        let adapter = fixture
            .binary
            .parent()
            .unwrap()
            .join("examples/delegation-live-adapter.exe");
        assert!(
            adapter.is_file(),
            "build the qualification delegation-live-adapter example first"
        );
        let directory = fixture._temp.path().join("adapter");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(directory.join("children")).unwrap();
        let prompt = directory.join("prompt.txt");
        fs::write(
            &prompt,
            "Observe value.txt and report current evidence; no unsupported check claims.",
        )
        .unwrap();
        let mut profile: Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        if !generation {
            profile["checks"] = json!([]);
            profile["processes"] = json!([]);
        }
        fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        let delegation = directory.join("delegation.json");
        let git = PathBuf::from(std::env::var_os("VCP_TEST_GIT").unwrap());
        fs::write(&delegation,serde_json::to_vec(&json!({"version":1,"git":git,"disposable_parent":directory.join("children"),"objective":if generation {"Change value.txt to 42 and report checks that remain for parent"} else {"Review value.txt without editing"},"acceptance":["Observed evidence"],"mode":if generation {"isolated_write"} else {"read_only"},"write_paths":if generation {vec!["value.txt"]} else {vec![]},"untracked_inputs":["value.txt","package.json","acceptance.cjs"],"allocation_usd":"0.1","seconds":120,"required_checks":[]})).unwrap()).unwrap();
        let spec = directory.join("spec.json");
        let bytes=serde_json::to_vec(&json!({"profile":fixture.profile,"workspace":fixture.workspace,"directory":directory,"prompt":prompt,"delegation":delegation,"git":git,"generation":generation,"human_note":null})).unwrap();
        fs::write(&spec, &bytes).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(if generation {
                        response(index, "complete")
                    } else {
                        response(3, "complete")
                    })
            })
            .mount(&server)
            .await;
        let output = tokio::task::spawn_blocking(move || {
            Command::new(adapter)
                .args([spec.to_str().unwrap(), &vcp_protocol::digest_bytes(&bytes)])
                .env("OPENROUTER_API_KEY", "synthetic-cli-qualification")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        let report_bytes =
            fs::read(directory.join("adapter-result.json")).unwrap_or_else(|error| {
                panic!(
                    "adapter report missing: {error}; exit={:?}; stdout={}; stderr={}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            });
        let report: Value = serde_json::from_slice(&report_bytes).unwrap();
        assert!(
            output.status.success(),
            "{report}; {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let state: Value =
            serde_json::from_slice(&fs::read(directory.join("canonical-state.json")).unwrap())
                .unwrap();
        let attempts = state["records"]
            .as_object()
            .unwrap()
            .values()
            .filter(|r| r["collection"] == "attempt")
            .collect::<Vec<_>>();
        assert!(!attempts.is_empty());
        assert!(attempts
            .iter()
            .all(|r| r["value"]["role"] == "child" && r["value"]["phase"] == "settled"));
        if generation {
            assert!(report["result"]["integration"]["effect"].is_string());
            assert!(report["result"]["verification"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|check| check["outcome"]["status"] == "passed"));
        }
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            if generation { "42\n" } else { "41\n" }
        );
    }
}
