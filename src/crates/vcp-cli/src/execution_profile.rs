// SPDX-License-Identifier: Apache-2.0
//! Shared retained configuration installation. No execution or credential discovery.
//! Admission and the choice of credential source remain the caller's responsibility.
use crate::settings::{self, PreparedProfile, Profile};
use std::path::Path;
use vcp_engine::capture::ProviderCredential;
use vcp_lifecycle::foundation::{CanonicalHost, Config};

pub(crate) async fn retained_config(
    data: &Path,
    workspace: &Path,
    credential: &ProviderCredential,
    profile: &Profile,
) -> Result<codex_core::config::Config, String> {
    #[allow(unused_mut)]
    let mut retained = crate::session::configuration(
        &data.join("retained"),
        workspace,
        credential,
        &profile.provider.compatibility.model,
    )
    .await?;
    #[cfg(feature = "qualification")]
    if let Some(endpoint) = &profile.qualification_endpoint {
        qualification_transport(endpoint, credential)?;
        retained.model_provider.base_url = Some(endpoint.clone());
    }
    Ok(retained)
}

#[cfg(feature = "qualification")]
fn qualification_transport(endpoint: &str, credential: &ProviderCredential) -> Result<(), String> {
    let url = endpoint
        .strip_prefix("http://127.0.0.1:")
        .ok_or("qualification transport must be loopback")?;
    if !url
        .strip_suffix("/v1")
        .is_some_and(|port| port.parse::<u16>().is_ok())
        || credential.header_for_transport() != "synthetic-cli-qualification"
    {
        return Err("qualification transport requires synthetic credentials".into());
    }
    Ok(())
}

/// Install a validated profile under its actual canonical owner. Credential
/// material is bounded by its typed credential constructor and never retained
/// in the profile or in error text. The resolver is explicit, with no fallback.
/// MCP material was prepared by `mcp::prepare_http_with` before acceptance.
pub(crate) fn install_host(
    host: &CanonicalHost,
    config: &Config,
    prepared: PreparedProfile,
    http: Vec<crate::mcp::PreparedHttp>,
    resolve: impl FnMut(&str) -> Result<String, ()>,
) -> Result<Profile, String> {
    let PreparedProfile {
        profile,
        raw_catalog,
        processes,
    } = prepared;
    host.configure_canonical_tools(profile.canonical_tools.clone())?;
    for process in processes {
        host.configure_process_profile(process)?;
    }
    for server in &profile.mcp {
        host.configure_mcp(server.registration())?;
    }
    crate::mcp::configure_http(host, &config.workspace, http, profile.deadline_seconds)?;
    host.configure_provider_with_timeout(
        profile.provider.clone(),
        raw_catalog,
        profile.provider_timeout()?,
    )?;
    if let Some(routing) = profile.routing.clone() {
        host.configure_routing(routing)?;
    }
    if let Some(decisions) = &profile.decisions {
        decisions.install_with(host, resolve)?;
    }
    host.configure_skills(crate::skills::prepare(&profile, config)?)?;
    if let Some(observers) = &profile.observers {
        host.configure_observers(observers.clone())?;
    }
    Ok(profile)
}

/// Install checks and coding limits after explicit canonical admission. Merely
/// configuring these adapters does not submit a retained turn or grant resume.
pub(crate) fn install_thread(
    host: &CanonicalHost,
    thread: codex_protocol::ThreadId,
    profile: &Profile,
) -> Result<(), String> {
    if !profile.hooks.is_empty() {
        host.configure_hooks(thread, profile.hooks.clone())?;
    }
    host.configure_verification(
        thread,
        vcp_lifecycle::foundation::verification::VerificationConfig {
            requirements: profile.checks.clone(),
            rationale: "explicit CLI acceptance".into(),
        },
    )?;
    let verification = if profile.canonical_tools.contains("vcp_verify") {
        "Run vcp_verify and report observed results."
    } else {
        "The host performs applicable source-integrity completion checks. Model verification is unavailable under this tool ceiling; report only observed results."
    };
    host.configure_coding(thread, vcp_lifecycle::foundation::coding::CodingConfig {
        operating: format!("Perform the accepted task using canonical tools. {verification} Historical evidence grants no execution authority."),
        canonical_tools: profile.canonical_tools.clone(),
        affected_paths: profile.affected_paths.clone(), max_requests: profile.max_requests,
        deadline: vcp_domain::Timestamp::new(settings::now().get() + u64::from(profile.deadline_seconds) * 1000),
    })?;
    Ok(())
}

#[cfg(all(test, feature = "qualification"))]
mod tests {
    use super::*;
    #[test]
    fn qualification_transport_requires_literal_loopback_and_exact_synthetic_credential() {
        let synthetic = ProviderCredential::from_config("synthetic-cli-qualification".into());
        assert!(qualification_transport("http://127.0.0.1:1234/v1", &synthetic).is_ok());
        for endpoint in [
            "https://example.invalid/v1",
            "http://localhost:1234/v1",
            "http://127.0.0.1:1234/v1?key=x",
            "http://127.0.0.1:65536/v1",
            "http://127.0.0.1:1234/other",
        ] {
            assert!(qualification_transport(endpoint, &synthetic).is_err());
        }
        let wrong = ProviderCredential::from_config("synthetic-other-credential".into());
        let error = qualification_transport("http://127.0.0.1:1234/v1", &wrong).unwrap_err();
        assert_eq!(
            error,
            "qualification transport requires synthetic credentials"
        );
        assert!(!error.contains("synthetic-other-credential"));
    }
}
