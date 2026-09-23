// SPDX-License-Identifier: Apache-2.0
//! Retained-controller startup for the CLI. No alternate model/tool loop.
use codex_core::{config::Config, CodexThread, StartThreadOptions, ThreadManager};
use codex_extension_api::{
    ExtensionRegistryBuilder, LoadedUserInstructions, UserInstructionsProvider,
};
use std::{path::Path, sync::Arc};
use vcp_lifecycle::foundation::{CanonicalHost, ThreadBinding};

// The VCP host supplies captured, scoped instructions at request assembly.
struct CanonicalInstructions;
impl UserInstructionsProvider for CanonicalInstructions {
    fn load_user_instructions(&self) -> codex_extension_api::LoadInstructionsFuture<'_> {
        Box::pin(async { LoadedUserInstructions::default() })
    }
}

#[derive(Clone)]
pub struct Session {
    pub manager: Arc<ThreadManager>,
    pub thread: Arc<CodexThread>,
    pub id: codex_protocol::ThreadId,
    events: Arc<tokio::sync::Mutex<()>>,
    scope: vcp_domain::workspace::Scope,
}

enum Startup<'a> {
    Internal,
    Public {
        connection: &'a vcp_lifecycle::foundation::PublicConnection,
        ticket: vcp_lifecycle::foundation::PublicResumeStartup,
        current: &'a vcp_engine::Access,
    },
    PublicStart {
        connection: &'a vcp_lifecycle::foundation::PublicConnection,
        ticket: vcp_lifecycle::foundation::PublicStartStartup,
        current: &'a vcp_engine::Access,
    },
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
        .await
        .map_err(|e| format!("retained configuration: {e}"))?;
    config.cwd = workspace
        .to_path_buf()
        .try_into()
        .map_err(|_| "absolute workspace required")?;
    vcp_lifecycle::foundation::openrouter::configure_transport(&mut config, credential, model)?;
    Ok(config)
}

impl Session {
    pub fn scope(&self) -> &vcp_domain::workspace::Scope {
        &self.scope
    }
    pub(crate) fn claim_events(&self) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
        self.events
            .clone()
            .try_lock_owned()
            .map_err(|_| "retained session already has an event owner".into())
    }
    /// Reattach a registered child while held. Explicit resume remains a
    /// separate control after canonical/native reconciliation.
    pub async fn recover_child(
        &self,
        host: &CanonicalHost,
        child: vcp_domain::TaskId,
        snapshotter: &vcp_repository::worktree::Snapshotter,
    ) -> Result<(Self, vcp_domain::ArtifactId), String> {
        let scope = vcp_domain::workspace::Scope {
            task: child.clone(),
            ..self.scope.clone()
        };
        let recovery = host
            .prepare_child_recovery(self.id, child, snapshotter)
            .await?;
        let evidence = recovery.evidence;
        let mut ticket = recovery.ticket.ok_or_else(|| {
            format!(
                "child recovery blocked: {}; evidence={evidence}",
                recovery
                    .blocked
                    .unwrap_or_else(|| "no attachment ticket".into())
            )
        })?;
        let mut config = self.thread.config().await.as_ref().clone();
        config.cwd = ticket
            .workspace()
            .to_path_buf()
            .try_into()
            .map_err(|_| "absolute child workspace required")?;
        host.authorize_child_recovery_startup(&mut ticket)?;
        let mut options = StartThreadOptions::new(config);
        options
            .thread_extension_init
            .insert(vcp_lifecycle::foundation::coding::allowed_tools());
        let started = self
            .manager
            .start_thread(options)
            .await
            .map_err(|e| format!("retained recovery startup: {e}"))?;
        let id = match host
            .attach_recovered_child(ticket, started.thread.clone())
            .await
        {
            Ok(id) => id,
            Err(error) => {
                let cleanup = started.thread.shutdown_and_wait().await;
                return Err(format!("{error}; retained startup cleanup: {cleanup:?}"));
            }
        };
        Ok((
            Self {
                manager: self.manager.clone(),
                thread: started.thread,
                id,
                events: Arc::new(tokio::sync::Mutex::new(())),
                scope,
            },
            evidence,
        ))
    }
    /// Start an already admitted, isolated child on the same retained manager.
    /// The canonical host owns lineage, authority, accounting and launch order.
    pub async fn start_child(
        &self,
        host: &CanonicalHost,
        mut config: Config,
        child: vcp_domain::TaskId,
        snapshotter: &vcp_repository::worktree::Snapshotter,
    ) -> Result<Self, String> {
        let scope = vcp_domain::workspace::Scope {
            task: child.clone(),
            ..self.scope.clone()
        };
        let ticket = host
            .prepare_child_start(self.id, child, snapshotter)
            .await?;
        config.cwd = ticket
            .workspace()
            .to_path_buf()
            .try_into()
            .map_err(|_| "absolute child workspace required")?;
        host.lifecycle()
            .authorize_startup(config.cwd.as_path(), None)
            .map_err(|e| format!("child startup: {e:?}"))?;
        let mut options = StartThreadOptions::new(config);
        options
            .thread_extension_init
            .insert(vcp_lifecycle::foundation::coding::allowed_tools());
        let started = self
            .manager
            .start_thread(options)
            .await
            .map_err(|e| format!("retained child startup: {e}"))?;
        let id = match host.attach_prepared_child(ticket, started.thread.clone()) {
            Ok(id) => id,
            Err(error) => {
                let cleanup = started.thread.shutdown_and_wait().await;
                return Err(format!("{error}; retained startup cleanup: {cleanup:?}"));
            }
        };
        Ok(Self {
            manager: self.manager.clone(),
            thread: started.thread,
            id,
            events: Arc::new(tokio::sync::Mutex::new(())),
            scope,
        })
    }
    /// Caller has validated configuration and created the canonical task before
    /// startup. Turn admission remains closed until that task is bound below.
    pub async fn start(
        host: &CanonicalHost,
        config: Config,
        binding: ThreadBinding,
    ) -> Result<Self, String> {
        Self::start_with(host, config, binding, Startup::Internal).await
    }

    /// Construct only the root authorized by an explicit public resume ticket.
    /// Attachment remains locally held until the separate durable resume commit.
    pub async fn start_public(
        host: &CanonicalHost,
        config: Config,
        binding: ThreadBinding,
        connection: &vcp_lifecycle::foundation::PublicConnection,
        startup: vcp_lifecycle::foundation::PublicResumeStartup,
        current: &vcp_engine::Access,
    ) -> Result<Self, String> {
        Self::start_with(
            host,
            config,
            binding,
            Startup::Public {
                connection,
                ticket: startup,
                current,
            },
        )
        .await
    }

    /// Construct for a newly accepted public run. Replay cannot mint this grant.
    pub async fn start_public_run(
        host: &CanonicalHost,
        config: Config,
        binding: ThreadBinding,
        connection: &vcp_lifecycle::foundation::PublicConnection,
        startup: vcp_lifecycle::foundation::PublicStartStartup,
        current: &vcp_engine::Access,
    ) -> Result<Self, String> {
        Self::start_with(
            host,
            config,
            binding,
            Startup::PublicStart {
                connection,
                ticket: startup,
                current,
            },
        )
        .await
    }

    async fn start_with(
        host: &CanonicalHost,
        config: Config,
        binding: ThreadBinding,
        startup: Startup<'_>,
    ) -> Result<Self, String> {
        let scope = binding.scope.clone();
        let auth = Arc::new(
            codex_login::AuthManager::new(
                config.codex_home.to_path_buf(),
                false,
                codex_config::types::AuthCredentialsStoreMode::Ephemeral,
                None,
                None,
                Default::default(),
                codex_login::AuthRouteConfig::from_http_client_factory(
                    config.http_client_factory(),
                ),
            )
            .await,
        );
        let models = codex_core::build_models_manager(&config, auth.clone());
        // Native tools are the VCP broker's responsibility. Retained remote exec
        // environment discovery must not import environment-variable servers.
        let environments = Arc::new(codex_exec_server::EnvironmentManager::without_environments(
            config.http_client_factory(),
        ));
        let mut extensions = ExtensionRegistryBuilder::new();
        extensions.turn_start_admission(Arc::new(host.clone()));
        extensions.work_admission(match &startup {
            Startup::Internal => Arc::new(host.clone()),
            Startup::Public { ticket, .. } => ticket.work_admission(),
            Startup::PublicStart { ticket, .. } => ticket.work_admission(),
        });
        extensions.tool_contributor(Arc::new(host.clone()));
        let manager = Arc::new(ThreadManager::new(
            &config,
            auth,
            models,
            Default::default(),
            codex_protocol::protocol::SessionSource::Exec,
            environments,
            Arc::new(extensions.build()),
            Arc::new(CanonicalInstructions),
            None,
            codex_core::passthrough_image_store(),
            codex_core::thread_store_from_config(&config, None),
            None,
            binding.scope.workspace.to_string(),
            None,
            None,
        ));
        if matches!(&startup, Startup::Internal) {
            host.lifecycle()
                .authorize_startup(config.cwd.as_path(), None)
                .map_err(|e| format!("owner startup: {e:?}"))?;
        }
        let mut options = StartThreadOptions::new(config);
        options
            .thread_extension_init
            .insert(vcp_lifecycle::foundation::coding::allowed_tools());
        let started = manager
            .start_thread(options)
            .await
            .map_err(|e| format!("retained startup: {e}"))?;
        let attached = match startup {
            Startup::Internal => host
                .lifecycle()
                .attach_root(started.thread.clone())
                .map_err(|e| format!("owner attachment: {e:?}"))
                .and_then(|id| host.register(id, binding).map(|_| id)),
            Startup::Public {
                connection,
                ticket,
                current,
            } => connection.attach_resume_root(ticket, started.thread.clone(), binding, current),
            Startup::PublicStart {
                connection,
                ticket,
                current,
            } => connection.attach_start_root(ticket, started.thread.clone(), binding, current),
        };
        let id = match attached {
            Ok(id) => id,
            Err(error) => {
                let cleanup = started.thread.shutdown_and_wait().await;
                return Err(format!("{error}; retained startup cleanup: {cleanup:?}"));
            }
        };
        Ok(Self {
            manager,
            thread: started.thread,
            id,
            events: Arc::new(tokio::sync::Mutex::new(())),
            scope,
        })
    }
}
