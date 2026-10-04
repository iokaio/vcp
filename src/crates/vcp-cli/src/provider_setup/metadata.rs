// SPDX-License-Identifier: Apache-2.0
//! Fresh endpoint metadata bound to the current adapter, without inference.
use super::*;
use vcp_models::catalog::{compatibility, Snapshot};

#[derive(Debug, clap::Args)]
pub struct Metadata {
    /// Exact selected OpenRouter model ID.
    #[arg(long)]
    pub model: String,
    /// Exact selected endpoint tag; no alternative endpoint is chosen.
    #[arg(long)]
    pub endpoint: String,
    /// New private generation directory; existing evidence is never overwritten.
    #[arg(long)]
    pub output: PathBuf,
}

fn selection(request: &Metadata) -> Result<()> {
    let exact = |value: &str| {
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-._/".contains(&c))
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
    };
    if !exact(&request.model) || !exact(&request.endpoint) {
        return Err("bounded exact model ID and endpoint tag required".into());
    }
    Ok(())
}

fn captured(request: &Metadata, raw: &[u8], observed: Timestamp) -> Result<Snapshot> {
    // Previous snapshots supply no evidence here. Admission derives exclusively
    // from the compiled adapter and newly fetched complete endpoint metadata.
    compatibility::snapshots(raw, observed)?
        .into_iter()
        .find(|snapshot| {
            snapshot.compatibility.model == request.model
                && snapshot.compatibility.endpoint == request.endpoint
        })
        .ok_or_else(|| {
            "fresh metadata does not support the selected exact model and endpoint".into()
        })
}

pub async fn run(
    request: &Metadata,
    workspace: &Path,
) -> std::result::Result<serde_json::Value, String> {
    run_with(request, workspace, "https://openrouter.ai/api/v1", now())
        .await
        .map_err(|error| error.to_string())
}

// URL injection is private and used only by offline transport tests.
async fn run_with(
    request: &Metadata,
    workspace: &Path,
    api: &str,
    observed: Timestamp,
) -> Result<serde_json::Value> {
    selection(request)?;
    let output = crate::settings::local_path(&request.output, workspace)?;
    reject_links(&output)?;
    std::fs::create_dir(&output).map_err(|_| {
        "metadata output must be a new private directory beneath an existing parent"
    })?;
    let root = crate::settings::registry_root(&output)?;
    let _pin = root.hold(None, true)?;
    fresh_file(
        &output.join("metadata-source.json"),
        &json!({"model":request.model,"endpoint":request.endpoint,
            "adapter_evidence_id":compatibility::evidence_id(),"model_calls":0,
            "operation":"public endpoint metadata GET only; current compiled adapter"}),
    )?;
    let client = production::client()?;
    let mut url = reqwest::Url::parse(&format!("{api}/models/"))?;
    url.path_segments_mut()
        .map_err(|_| "provider catalog URL")?
        .pop_if_empty()
        .extend(request.model.split('/'))
        .push("endpoints");
    let raw = production::get(&client, url, None).await?;
    production::fresh_bytes(&output.join("endpoints.json"), &raw)?;
    let snapshot = match captured(request, &raw, observed) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            fresh_file(
                &output.join("rejected-metadata.json"),
                &json!({"status":"rejected","reason":error.to_string(),"model_calls":0}),
            )?;
            return Err(error);
        }
    };
    fresh_file(&output.join("snapshot.json"), &snapshot)?;
    let report = json!({"status":"created","generation":output,
        "snapshot":output.join("snapshot.json"),"catalog":output.join("endpoints.json"),
        "model":snapshot.compatibility.model,"endpoint":snapshot.compatibility.endpoint,
        "snapshot_id":snapshot.id,"valid_until":snapshot.valid_until,"model_calls":0});
    fresh_file(&output.join("result.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> serde_json::Value {
        json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region",
            "status":0,"context_length":32000,"max_prompt_tokens":24000,
            "max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],
            "pricing":{"prompt":"0.000001","completion":"0.000002","request":"0"}}]}})
    }
    fn request(output: PathBuf) -> Metadata {
        Metadata {
            model: "fixture/model".into(),
            endpoint: "fixture/region".into(),
            output,
        }
    }

    #[test]
    fn cli_requires_exact_selection_without_credentials_budget_or_retained_evidence() {
        use clap::Parser;
        let args = [
            "vcp",
            "setup",
            "provider-metadata",
            "--model",
            "fixture/model",
            "--endpoint",
            "fixture/region",
            "--output",
            "new-generation",
        ];
        assert!(crate::args::Cli::try_parse_from(args).is_ok());
        for forbidden in ["--budget-usd", "--api-key", "--snapshot", "--catalog"] {
            assert!(
                crate::args::Cli::try_parse_from(args.into_iter().chain([forbidden, "value"]))
                    .is_err()
            );
        }
    }

    #[test]
    fn current_adapter_metadata_rejects_wrong_unsupported_and_ambiguous_identity() {
        let request = request("unused".into());
        let valid = captured(
            &request,
            &serde_json::to_vec(&raw()).unwrap(),
            compatibility::REVIEWED_AT,
        )
        .unwrap();
        assert_eq!(valid.compatibility.id, compatibility::evidence_id());
        assert!(!valid.compatibility.responses_text_tools);
        assert!(!valid.compatibility.provider_preferences_qualified);
        assert!(!valid.compatibility.byte_ceiling_qualified);
        let mut old = valid.clone();
        old.compatibility.id =
            format!("openrouter-responses-adapter-contract/1/{}", "0".repeat(64));
        // A metadata timestamp that is still current cannot admit old code.
        old.current(compatibility::REVIEWED_AT).unwrap();
        assert!(Snapshot::from_endpoints(
            &serde_json::to_vec(&raw()).unwrap(),
            old.observed_at,
            old.valid_until,
            old.compatibility,
        )
        .is_err());
        for field in ["tag", "status", "supported_parameters", "pricing"] {
            let mut changed = raw();
            changed["data"]["endpoints"][0][field] = json!("different");
            assert!(captured(
                &request,
                &serde_json::to_vec(&changed).unwrap(),
                compatibility::REVIEWED_AT
            )
            .is_err());
        }
        let mut changed = raw();
        changed["data"]["id"] = json!("another/model");
        assert!(captured(
            &request,
            &serde_json::to_vec(&changed).unwrap(),
            compatibility::REVIEWED_AT
        )
        .is_err());
        changed = raw();
        let duplicate = changed["data"]["endpoints"][0].clone();
        changed["data"]["endpoints"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(captured(
            &request,
            &serde_json::to_vec(&changed).unwrap(),
            compatibility::REVIEWED_AT
        )
        .is_err());
        assert!(captured(
            &request,
            &serde_json::to_vec(&raw()).unwrap(),
            compatibility::VALID_UNTIL
        )
        .is_err());
    }

    #[tokio::test]
    async fn invalid_selection_fails_before_output_or_network() {
        let temp = tempfile::tempdir().unwrap();
        let mut oversized = request(temp.path().join("not-created"));
        oversized.model = "a".repeat(257);
        assert!(run_with(
            &oversized,
            temp.path(),
            "http://127.0.0.1:1",
            compatibility::REVIEWED_AT
        )
        .await
        .is_err());
        assert!(!oversized.output.exists());
        for invalid in [
            "",
            "../model",
            "fixture//model",
            "fixture/model?key=value",
            "fixture/模型",
        ] {
            for model in [true, false] {
                let mut request = request(temp.path().join("not-created"));
                if model {
                    request.model = invalid.into();
                } else {
                    request.endpoint = invalid.into();
                }
                assert!(run_with(
                    &request,
                    temp.path(),
                    "http://127.0.0.1:1",
                    compatibility::REVIEWED_AT
                )
                .await
                .is_err());
                assert!(!request.output.exists());
            }
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn public_transport_is_one_get_create_only_and_retains_rejected_catalogs() {
        use std::io::{Read, Write};
        for accepted in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let request = request(temp.path().join("metadata"));
            let mut catalog = raw();
            if !accepted {
                catalog["data"]["id"] = json!("another/model");
            }
            let raw = serde_json::to_vec(&catalog).unwrap();
            let server_raw = raw.clone();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let api = format!("http://{}/api/v1", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 8192);
                }
                let received = String::from_utf8(bytes).unwrap();
                assert!(received.starts_with("GET /api/v1/models/fixture/model/endpoints HTTP/1.1"));
                assert!(!received.to_ascii_lowercase().contains("authorization:"));
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", server_raw.len()).unwrap();
                stream.write_all(&server_raw).unwrap();
            });
            let result = run_with(&request, &workspace, &api, compatibility::REVIEWED_AT).await;
            server.join().unwrap();
            assert_eq!(
                std::fs::read(request.output.join("endpoints.json")).unwrap(),
                raw
            );
            assert!(request.output.join("metadata-source.json").is_file());
            if accepted {
                let result = result.unwrap();
                assert_eq!(result["status"], "created");
                assert_eq!(result["model_calls"], 0);
                let snapshot: Snapshot = serde_json::from_slice(
                    &std::fs::read(request.output.join("snapshot.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(snapshot.compatibility.id, compatibility::evidence_id());
                snapshot.current(compatibility::REVIEWED_AT).unwrap();
            } else {
                assert!(result.is_err());
                assert!(request.output.join("rejected-metadata.json").is_file());
                assert!(!request.output.join("snapshot.json").exists());
            }
            // The server is gone. Reuse must fail before requesting any metadata.
            assert!(
                run_with(&request, &workspace, &api, compatibility::REVIEWED_AT)
                    .await
                    .is_err()
            );
            assert_eq!(
                std::fs::read(request.output.join("endpoints.json")).unwrap(),
                raw
            );
        }
    }
}
