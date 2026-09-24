// SPDX-License-Identifier: Apache-2.0
//! A fresh accepted run owns construction and submission. Replay owns neither.
use super::*;
use vcp_lifecycle::foundation::{PublicStartAdmission, PublicStartOutcome, PublicStartTicket};

impl Supervisor {
    pub(super) async fn start(
        &self,
        connection: &mut PublicConnection,
        request: methods::TurnStart,
        current: &Access,
    ) -> Result<CommandReceipt, RpcError> {
        let mut state = self.state.lock().await;
        let operation = request.mutation.command_id.clone();
        if state.closed || request.task.as_str() != self.config.root_task.as_str() {
            return Err(failure(Code::PolicyDenied, Some(operation)));
        }
        let preparation = match connection.prepare_start_rpc(request.clone(), current)? {
            PublicStartAdmission::Replay(receipt) => return Ok(receipt),
            PublicStartAdmission::Ready(preparation) => preparation,
        };
        // Only a selected absent root can start. Existing execution belongs to
        // its own explicit resume flow and cannot be replaced by a new request.
        if state.session.is_some() || state.pump.is_some() || state.deadline.is_some() {
            return Err(failure(Code::VersionConflict, Some(operation)));
        }
        let (prepared, policy) = self
            .prepare_profile(state.policy.as_deref())
            .map_err(|_| failure(Code::PolicyDenied, Some(operation.clone())))?;
        let profile = &prepared.profile;
        if self.config.cap.currency.code() != "USD"
            || request.budget.currency != methods::Currency::Usd
            || request.budget.cap_micros.as_str() != self.config.cap.micros.get().to_string()
            || request.budget.max_requests != profile.max_requests
            || request.budget.deadline_seconds != profile.deadline_seconds
        {
            return Err(failure(Code::PolicyDenied, Some(operation)));
        }
        let http = crate::mcp::prepare_http_with(&profile.mcp_http, |name| {
            self.configuration.credentials.get(name).cloned().ok_or(())
        })
        .map_err(|_| failure(Code::PolicyDenied, Some(operation.clone())))?;
        let _deadline = ProcessDeadline::start(Some(connection.loss_signal()))
            .map_err(|_| failure(Code::StoreUnavailable, Some(operation.clone())))?;
        let accepted = connection.accept_start(
            preparation,
            current,
            !profile.checks.is_empty(),
            profile
                .checks
                .iter()
                .map(|check| format!("{}#test", check.manifest))
                .collect(),
        )?;
        let (receipt, ticket) = match accepted {
            PublicStartOutcome::Replay(receipt) => return Ok(receipt),
            PublicStartOutcome::Accepted { receipt, ticket } => (receipt, ticket),
        };
        let scope = ticket.scope().clone();
        // Everything after durable acceptance is owned. A construction failure
        // is visible as paused work with the original accepted receipt, not an
        // invitation to submit the same command again.
        let launched = self
            .start_accepted(
                &mut state,
                connection,
                ticket,
                current,
                prepared,
                http,
                policy,
                request.turn,
            )
            .await;
        if launched.is_err() {
            pause(&self.host, &scope);
        }
        Ok(receipt)
    }

    async fn start_accepted(
        &self,
        state: &mut State,
        connection: &mut PublicConnection,
        mut ticket: PublicStartTicket,
        current: &Access,
        prepared: crate::settings::PreparedProfile,
        http: Vec<crate::mcp::PreparedHttp>,
        policy: String,
        turn: methods::Id,
    ) -> Result<(), String> {
        let scope = ticket.scope().clone();
        let credential = vcp_engine::capture::ProviderCredential::from_config(
            self.configuration.provider_credential.clone(),
        );
        let retained = crate::execution_profile::retained_config(
            &self.data,
            std::path::Path::new(&self.config.binding.root),
            &credential,
            &prepared.profile,
        )
        .await?;
        let profile = crate::execution_profile::install_host(
            &self.host,
            &self.config,
            prepared,
            http,
            |name| self.configuration.credentials.get(name).cloned().ok_or(()),
        )?;
        let startup = connection.authorize_start_startup(&mut ticket, current)?;
        let binding = ThreadBinding {
            scope: scope.clone(),
            agent: AgentId::new(),
            role: vcp_domain::accounting::RequestRole::Main,
        };
        state.session = Some(
            crate::session::Session::start_public_run(
                &self.host, retained, binding, connection, startup, current,
            )
            .await?,
        );
        state.policy = Some(policy);
        let session = state
            .session
            .as_ref()
            .ok_or("execution session unavailable")?;
        let execution = crate::execution::RetainedExecution::claim(&self.host, session, &scope)?;
        connection.activate_start(ticket, current)?;
        crate::execution_profile::install_thread(&self.host, session.id, &profile)?;
        state.configured = true;
        let expires = self.execution_expiry(&profile)?;
        state.deadline = Some(expires);
        let turn = TurnId::parse(turn.as_str()).map_err(|_| "invalid accepted turn")?;
        state.pump = Some(pump(
            self.host.clone(),
            scope,
            execution,
            Some(turn),
            expires,
        ));
        Ok(())
    }
}
