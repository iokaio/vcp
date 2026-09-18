// SPDX-License-Identifier: Apache-2.0
use vcp_engine::capture::ProviderCredential;

/// Explicit transport setup for the retained client. The canonical provider
/// gateway must also be configured and installed as its work-admission host.
/// This helper performs no request, secret discovery, or credential persistence.
pub fn configure_transport(
    config: &mut codex_core::config::Config,
    credential: &ProviderCredential,
    model: &str,
) -> Result<(), String> {
    let token = credential.header_for_transport();
    if token.is_empty() || token.len() > 4096 || token.bytes().any(|b| !b.is_ascii_graphic()) {
        return Err("explicit OpenRouter credential is missing or invalid".into());
    }
    if model.is_empty()
        || model.len() > 256
        || !model.contains('/')
        || !model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
    {
        return Err(
            "explicit qualified model slug required; router variants are not enabled".into(),
        );
    }
    if !config.mcp_servers.get().is_empty() {
        return Err("MCP requires the later governed adapter".into());
    }
    for feature in codex_features::FEATURES {
        config
            .features
            .disable(feature.id)
            .map_err(|_| "feature ceiling could not be installed")?;
    }
    config.analytics_enabled = Some(false);
    config.otel = Default::default();
    config.otel.metrics_exporter = config.otel.exporter.clone();
    config.notify = None;
    config.orchestrator_skills_enabled = false;
    config.orchestrator_mcp_enabled = false;
    // VCP's captured, scoped AGENTS.md loader is authoritative for this path.
    config.project_doc_max_bytes = 0;
    config.project_doc_fallback_filenames.clear();
    config.developer_instructions = None;
    config.model = Some(model.into());
    config.model_provider = Default::default();
    let provider = &mut config.model_provider;
    provider.name = "OpenRouter (explicit VCP gateway)".into();
    provider.base_url = Some("https://openrouter.ai/api/v1".into());
    provider.experimental_bearer_token = Some(token.to_owned().into());
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);
    provider.stream_idle_timeout_ms = Some(30_000);
    // Defaults disable login/AWS/command credentials, env discovery, query and
    // extra headers, standalone search, and WebSocket transport.
    Ok(())
}
