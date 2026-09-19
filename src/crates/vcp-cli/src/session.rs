// SPDX-License-Identifier: Apache-2.0
//! Retained-controller startup for the CLI. No alternate model/tool loop.
use codex_core::{config::Config, CodexThread, StartThreadOptions, ThreadManager};
use codex_extension_api::{ExtensionRegistryBuilder, LoadedUserInstructions, UserInstructionsProvider};
use std::{path::Path, sync::Arc};
use vcp_lifecycle::foundation::{CanonicalHost, ThreadBinding};

// The VCP host supplies captured, scoped instructions at request assembly.
struct CanonicalInstructions;
impl UserInstructionsProvider for CanonicalInstructions {
    fn load_user_instructions(&self) -> codex_extension_api::LoadInstructionsFuture<'_> {
        Box::pin(async { LoadedUserInstructions::default() })
    }
}

pub struct Session {
    pub manager: Arc<ThreadManager>,
    pub thread: Arc<CodexThread>,
    pub id: codex_protocol::ThreadId,
}

/// Load retained defaults without importing Codex user/project/system settings.
/// Credential material is transport-only and never serialized by this adapter.
pub async fn configuration(
    home: &Path,
    workspace: &Path,
    credential: &vcp_engine::capture::ProviderCredential,
    model: &str,
) -> Result<Config, String> {
    let mut config = Config::load_default_with_cli_overrides_for_codex_home(home.into(), vec![])
        .await.map_err(|e| format!("retained configuration: {e}"))?;
    config.cwd = workspace.to_path_buf().try_into().map_err(|_| "absolute workspace required")?;
    vcp_lifecycle::foundation::openrouter::configure_transport(&mut config, credential, model)?;
    Ok(config)
}

impl Session {
    /// Caller has validated configuration and created the canonical task before
    /// startup. Turn admission remains closed until that task is bound below.
    pub async fn start(host: &CanonicalHost, config: Config, binding: ThreadBinding) -> Result<Self, String> {
        let auth = Arc::new(codex_login::AuthManager::new(
            config.codex_home.to_path_buf(), false,
            codex_config::types::AuthCredentialsStoreMode::Ephemeral,
            None, None, Default::default(),
            codex_login::AuthRouteConfig::from_http_client_factory(config.http_client_factory()),
        ).await);
        let models = codex_core::build_models_manager(&config, auth.clone());
        // Native tools are the VCP broker's responsibility. Retained remote exec
        // environment discovery must not import environment-variable servers.
        let environments = Arc::new(codex_exec_server::EnvironmentManager::without_environments(config.http_client_factory()));
        let mut extensions = ExtensionRegistryBuilder::new();
        extensions.turn_start_admission(Arc::new(host.clone()));
        extensions.work_admission(Arc::new(host.clone()));
        extensions.tool_contributor(Arc::new(host.clone()));
        let manager = Arc::new(ThreadManager::new(
            &config, auth, models, Default::default(), codex_protocol::protocol::SessionSource::Exec,
            environments, Arc::new(extensions.build()), Arc::new(CanonicalInstructions), None,
            codex_core::passthrough_image_store(), codex_core::thread_store_from_config(&config, None),
            None, binding.scope.workspace.to_string(), None, None,
        ));
        host.lifecycle().authorize_startup(config.cwd.as_path(), None).map_err(|e| format!("owner startup: {e:?}"))?;
        let mut options = StartThreadOptions::new(config);
        options.thread_extension_init.insert(vcp_lifecycle::foundation::coding::allowed_tools());
        let started = manager.start_thread(options).await.map_err(|e| format!("retained startup: {e}"))?;
        let id = host.lifecycle().attach_root(started.thread.clone()).map_err(|e| format!("owner attachment: {e:?}"))?;
        host.register(id, binding)?;
        Ok(Self { manager, thread: started.thread, id })
    }
}
