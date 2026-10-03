// SPDX-License-Identifier: Apache-2.0
//! ADR-081 metadata renewal. Never dispatch inference or extend adapter evidence.
use super::*;
use vcp_models::catalog::{compatibility, Snapshot};

#[derive(Debug, clap::Args)]
pub struct Refresh {
    /// Retained adapter-contract snapshot identifying the exact model/endpoint.
    #[arg(long)]
    pub snapshot: PathBuf,
    /// Complete captured catalog matching the retained snapshot.
    #[arg(long)]
    pub catalog: PathBuf,
    /// New private generation directory; existing evidence is never overwritten.
    #[arg(long)]
    pub output: PathBuf,
}

fn retained(snapshot: &Snapshot, raw: &[u8], observed: Timestamp) -> Result<()> {
    if snapshot.compatibility.id != compatibility::evidence_id()
        || !compatibility::admitted(&snapshot.compatibility)
        || observed < snapshot.observed_at
        || observed >= snapshot.compatibility.valid_until
    {
        return Err("metadata-only refresh requires the current compiled adapter contract; empirical qualification or expired/altered adapter evidence cannot be renewed this way".into());
    }
    let captured = Snapshot::from_endpoints(
        raw,
        snapshot.observed_at,
        snapshot.valid_until,
        snapshot.compatibility.clone(),
    )?;
    if &captured != snapshot {
        return Err("retained snapshot differs from its captured endpoint catalog".into());
    }
    Ok(())
}

fn refreshed(snapshot: &Snapshot, raw: &[u8], observed: Timestamp) -> Result<Snapshot> {
    // Reparse complete prices, limits and capabilities; never copy old tariffs
    // or merely advance the previous snapshot's timestamps.
    compatibility::snapshots(raw, observed)?
        .into_iter()
        .find(|candidate| candidate.compatibility == snapshot.compatibility)
        .ok_or_else(|| {
            "fresh metadata does not support the retained exact model and endpoint".into()
        })
}

pub async fn run(
    request: &Refresh,
    workspace: &Path,
) -> std::result::Result<serde_json::Value, String> {
    run_with(request, workspace, "https://openrouter.ai/api/v1", now())
        .await
        .map_err(|error| error.to_string())
}

// Private URL injection is exclusively for offline transport tests. Production
// uses the fixed public metadata origin without credentials, retries or redirects.
async fn run_with(
    request: &Refresh,
    workspace: &Path,
    api: &str,
    observed: Timestamp,
) -> Result<serde_json::Value> {
    let snapshot_path = crate::settings::local_path(&request.snapshot, workspace)?;
    let catalog_path = crate::settings::local_path(&request.catalog, workspace)?;
    let snapshot: Snapshot =
        serde_json::from_slice(&crate::settings::read_bounded(&snapshot_path, 256 * 1024)?)?;
    let prior_raw = crate::settings::read_bounded(&catalog_path, 4 * 1024 * 1024)?;
    retained(&snapshot, &prior_raw, observed)?;
    let output = crate::settings::local_path(&request.output, workspace)?;
    reject_links(&output)?;
    std::fs::create_dir(&output).map_err(|_| {
        "metadata refresh output must be a new private directory beneath an existing parent"
    })?;
    let root = crate::settings::registry_root(&output)?;
    let _pin = root.hold(None, true)?;
    fresh_file(
        &output.join("refresh-source.json"),
        &json!({"snapshot":snapshot_path,"catalog":catalog_path,"snapshot_id":snapshot.id,
            "model":snapshot.compatibility.model,"endpoint":snapshot.compatibility.endpoint,
            "model_calls":0,"operation":"public endpoint metadata GET only"}),
    )?;
    let client = production::client()?;
    let mut url = reqwest::Url::parse(&format!("{api}/models/"))?;
    url.path_segments_mut()
        .map_err(|_| "provider catalog URL")?
        .pop_if_empty()
        .extend(snapshot.compatibility.model.split('/'))
        .push("endpoints");
    let raw = production::get(&client, url, None).await?;
    // Retain rejected catalogs too: they explain an unavailable/changed endpoint,
    // but only successful validation below publishes an admission snapshot.
    production::fresh_bytes(&output.join("endpoints.json"), &raw)?;
    let report = match refreshed(&snapshot, &raw, observed) {
        Ok(fresh) => {
            fresh_file(&output.join("snapshot.json"), &fresh)?;
            json!({"status":"refreshed","generation":output,"snapshot":output.join("snapshot.json"),
                    "catalog":output.join("endpoints.json"),"model":fresh.compatibility.model,
                    "endpoint":fresh.compatibility.endpoint,"prior_snapshot_id":snapshot.id,
                    "snapshot_id":fresh.id,"valid_until":fresh.valid_until,"model_calls":0})
        }
        Err(error) => {
            fresh_file(
                &output.join("rejected-metadata.json"),
                &json!({"status":"rejected","reason":error.to_string(),"model_calls":0}),
            )?;
            return Err(error);
        }
    };
    fresh_file(&output.join("result.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> serde_json::Value {
        json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],"pricing":{"prompt":"0.000001","completion":"0.000002","request":"0"}}]}})
    }
    #[test]
    fn cli_refresh_accepts_retained_paths_without_credentials_or_paid_budget() {
        use clap::Parser;
        let args = [
            "vcp",
            "setup",
            "provider-refresh",
            "--snapshot",
            "prior.json",
            "--catalog",
            "endpoints.json",
            "--output",
            "new-generation",
        ];
        assert!(crate::args::Cli::try_parse_from(args).is_ok());
        for forbidden in ["--budget-usd", "--api-key", "--model", "--endpoint"] {
            assert!(
                crate::args::Cli::try_parse_from(args.into_iter().chain([forbidden, "value"]))
                    .is_err()
            );
        }
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn metadata_transport_is_one_public_get_and_create_only_generation() {
        use std::io::{Read, Write};
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let raw = serde_json::to_vec(&raw()).unwrap();
        let old = compatibility::snapshots(&raw, compatibility::REVIEWED_AT)
            .unwrap()
            .remove(0);
        let snapshot = temp.path().join("snapshot.json");
        let catalog = temp.path().join("endpoints.json");
        std::fs::write(&snapshot, serde_json::to_vec(&old).unwrap()).unwrap();
        std::fs::write(&catalog, &raw).unwrap();
        let request = Refresh {
            snapshot,
            catalog,
            output: temp.path().join("refreshed"),
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /api/v1/models/fixture/model/endpoints HTTP/1.1"));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", raw.len()).unwrap();
            stream.write_all(&raw).unwrap();
        });
        let observed = Timestamp::new(old.valid_until.get() + 1000);
        let result = run_with(&request, &workspace, &api, observed)
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(result["status"], "refreshed");
        assert_eq!(result["model_calls"], 0);
        let new_bytes = std::fs::read(request.output.join("snapshot.json")).unwrap();
        let fresh: Snapshot = serde_json::from_slice(&new_bytes).unwrap();
        assert_eq!(fresh.compatibility, old.compatibility);
        assert_eq!(
            serde_json::from_slice::<Snapshot>(&std::fs::read(&request.snapshot).unwrap()).unwrap(),
            old
        );
        // Existing output rejection happens before a request (server is closed).
        assert!(run_with(&request, &workspace, &api, observed)
            .await
            .is_err());
        assert_eq!(
            std::fs::read(request.output.join("snapshot.json")).unwrap(),
            new_bytes
        );
    }
    #[test]
    fn expired_metadata_refreshes_prices_without_changing_adapter_or_selection() {
        let raw = serde_json::to_vec(&raw()).unwrap();
        let old = compatibility::snapshots(&raw, compatibility::REVIEWED_AT)
            .unwrap()
            .remove(0);
        let later = Timestamp::new(old.valid_until.get() + 1000);
        assert!(old.current(later).is_err());
        retained(&old, &raw, later).unwrap();
        let mut changed: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        changed["data"]["endpoints"][0]["pricing"]["prompt"] = json!("0.000009");
        let fresh = refreshed(&old, &serde_json::to_vec(&changed).unwrap(), later).unwrap();
        fresh.current(later).unwrap();
        assert_eq!(fresh.compatibility, old.compatibility);
        assert_ne!(fresh.id, old.id);
        assert_ne!(fresh.price, old.price);
        assert!(!fresh.compatibility.responses_text_tools);
        assert!(!fresh.compatibility.provider_preferences_qualified);
    }
    #[test]
    fn refresh_rejects_changed_identity_unsupported_metadata_and_forged_evidence() {
        let raw = serde_json::to_vec(&raw()).unwrap();
        let old = compatibility::snapshots(&raw, compatibility::REVIEWED_AT)
            .unwrap()
            .remove(0);
        let later = Timestamp::new(old.valid_until.get() + 1000);
        for field in ["tag", "status", "supported_parameters", "pricing"] {
            let mut changed: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            changed["data"]["endpoints"][0][field] = json!("different");
            assert!(refreshed(&old, &serde_json::to_vec(&changed).unwrap(), later).is_err());
        }
        let mut changed: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        changed["data"]["id"] = json!("another/model");
        assert!(refreshed(&old, &serde_json::to_vec(&changed).unwrap(), later).is_err());
        assert!(retained(&old, b"{}", later).is_err());
        let mut altered = old.clone();
        altered.compatibility.responses_text_tools = true;
        assert!(retained(&altered, &raw, later).is_err());
        altered = old.clone();
        altered.compatibility.id = "empirical-qualification".into();
        assert!(retained(&altered, &raw, later).is_err());
        altered = old.clone();
        altered.id.push('x');
        assert!(retained(&altered, &raw, later).is_err());
        assert!(retained(&old, &raw, compatibility::VALID_UNTIL).is_err());
    }
}
