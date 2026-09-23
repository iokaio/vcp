// SPDX-License-Identifier: Apache-2.0
//! Trusted canonical resume. This does not submit a retained model turn.
use super::public_connection::PublicConnection;
use super::*;
use codex_protocol::ThreadId;
use vcp_engine::public::PublicAdmission;
use vcp_protocol::methods::{Call, SessionResume};

enum Admission {
    Replay(CommandReceipt),
    Ready(ResumeCommit, Task),
}

impl PublicConnection {
    /// Revalidate and resume canonical execution with the caller's durable ID.
    /// Transport dispatch must separately own any subsequent retained submission.
    pub fn resume_canonical(
        &self,
        request: SessionResume,
        current: &Access,
    ) -> std::result::Result<CommandReceipt, String> {
        // Serialize durable replay with ephemeral MCP proof collection. This
        // lock never participates in connection-loss or lifecycle interruption.
        let _resume = self
            .host
            .public_resume
            .lock()
            .map_err(|_| "public resume lock poisoned")?;
        let (host, access, connection, token) = self.rpc_context(current)?;
        let token = token.ok_or("controller acquisition required")?;
        let admission = host.worker.run(move |context| {
            context.check_public_controller(&access, &connection, &token)?;
            let facts = HostFacts {
                now: now(),
                policy: vcp_engine::policy::optional(
                    context.engine.store().state(),
                    &context.config.workspace,
                )?
                .map_or(PolicyRevision::ZERO, |policy| policy.revision),
                resume: None,
                may_execute: context.owner_alive,
            };
            let prepared = match context.engine.prepare_controlled_public(
                Call::SessionResume(request),
                &access,
                &facts,
                &connection,
                &token,
            )? {
                PublicAdmission::Replay(receipt) => return Ok(Admission::Replay(receipt)),
                PublicAdmission::Ready(prepared) => prepared,
            };
            if context.authority_pending {
                return Err("authority change is stopping work".into());
            }
            let task: Task = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Task,
                    prepared.task().ok_or("resume task absent")?.as_str(),
                    &access.workspace,
                )?
                .decode()?;
            Ok(Admission::Ready(
                ResumeCommit::Public {
                    prepared,
                    access,
                    connection,
                    token,
                },
                task,
            ))
        })?;
        let (commit, task) = match admission {
            Admission::Replay(receipt) => return Ok(receipt),
            Admission::Ready(commit, task) => (commit, task),
        };
        // A reconnect or replay never constructs or rebinds a retained owner.
        let (thread, binding) = {
            let bindings = host.bindings.lock().map_err(|_| "binding lock poisoned")?;
            let mut selected = bindings
                .iter()
                .filter(|(_, binding)| binding.scope == task.scope);
            let (thread, binding) = selected
                .next()
                .ok_or("resume requires a retained task binding")?;
            if selected.next().is_some() {
                return Err("resume retained task binding is ambiguous".into());
            }
            (*thread, binding.clone())
        };
        let attempt = Arc::new(AtomicBool::new(false));
        let result = {
            #[cfg(windows)]
            if host.mcp_connections_present() {
                host.resume_mcp_with_commit(
                    thread,
                    binding,
                    task.revision,
                    task.fingerprint,
                    commit,
                    Some(attempt.clone()),
                )
            } else {
                resume_plain(&host, thread, binding, task, commit, attempt.clone())
            }
            #[cfg(not(windows))]
            resume_plain(&host, thread, binding, task, commit, attempt.clone())
        };
        if result.is_err() && attempt.load(Ordering::SeqCst) {
            // A store failure can have committed despite an error. Seal retained
            // dispatch before allowing command/read and restart reconciliation.
            host.worker.fence();
            drop(host.runtime.hold_owner());
        }
        result
    }
}

fn resume_plain(
    host: &super::super::CanonicalHost,
    thread: ThreadId,
    binding: ThreadBinding,
    task: Task,
    commit: ResumeCommit,
    attempt: Arc<AtomicBool>,
) -> std::result::Result<CommandReceipt, String> {
    let runtime = host.runtime.clone();
    host.worker.run(move |context| {
        context.resume_with_runtime(
            &binding,
            thread,
            &runtime,
            task.revision,
            task.fingerprint,
            commit,
            Some(attempt),
        )
    })
}

impl Context {
    pub(super) fn recheck_resume(&self, commit: &ResumeCommit) -> Result<Option<CommandReceipt>> {
        let ResumeCommit::Public {
            prepared,
            access,
            connection,
            token,
        } = commit
        else {
            return Ok(None);
        };
        self.check_public_controller(access, connection, token)?;
        let facts = HostFacts {
            now: now(),
            policy: vcp_engine::policy::optional(
                self.engine.store().state(),
                &self.config.workspace,
            )?
            .map_or(PolicyRevision::ZERO, |policy| policy.revision),
            resume: None,
            may_execute: self.owner_alive,
        };
        Ok(
            match self.engine.prepare_controlled_public(
                prepared.call().clone(),
                access,
                &facts,
                connection,
                token,
            )? {
                PublicAdmission::Replay(receipt) => Some(receipt),
                PublicAdmission::Ready(_) => {
                    if self.authority_pending {
                        return Err("authority change is stopping work".into());
                    }
                    None
                }
            },
        )
    }

    pub(in crate::foundation) fn resume_with_runtime(
        &mut self,
        binding: &ThreadBinding,
        thread: ThreadId,
        runtime: &crate::Lifecycle,
        expected: Revision,
        fingerprint: vcp_domain::verification::Fingerprint,
        commit: ResumeCommit,
        attempt: Option<Arc<AtomicBool>>,
    ) -> Result<CommandReceipt> {
        if matches!(commit, ResumeCommit::Internal) {
            return self.resume(binding, expected, fingerprint);
        }
        if let Some(receipt) = self.recheck_resume(&commit)? {
            return Ok(receipt);
        }
        // Serialized on the canonical worker, after authorized durable replay.
        self.reconcile_effects()?;
        self.resume_checked(binding, expected, fingerprint, &[], commit, || {
            let view = runtime
                .inspect(thread)
                .map_err(|error| format!("{error:?}"))?;
            if !view.owner_attached || view.inherited_hold || view.interruption_error.is_some() {
                return Err("retained owner is not ready for canonical resume".into());
            }
            if view.local_hold {
                runtime
                    .resume(thread, &view.revision)
                    .map_err(|error| format!("resume waits for interruption: {error:?}"))?;
                if let Some(attempt) = &attempt {
                    attempt.store(true, Ordering::SeqCst);
                }
            }
            let state = runtime
                .0
                .state
                .lock()
                .map_err(|_| "lifecycle poisoned during resume")?;
            if !state.attached
                || state.sealing
                || state.held(thread)
                || state.startups_in_flight != 0
                || state
                    .entries
                    .iter()
                    .any(|(id, entry)| state.below(*id, thread) && entry.starts != 0)
                || state
                    .work
                    .iter()
                    .any(|work| work.receipt.is_none() && state.below(work.thread, thread))
            {
                return Err("retained owner changed during resume observation".into());
            }
            if let Some(attempt) = attempt {
                attempt.store(true, Ordering::SeqCst);
            }
            Ok(state)
        })
    }
}
