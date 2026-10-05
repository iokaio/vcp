// SPDX-License-Identifier: Apache-2.0
//! Explicit credential-bound metadata-only retrieval for failed generations.
//! This transport cannot submit inference or change an approved route.
use std::{future::Future, pin::Pin, time::Duration};
use vcp_engine::capture::ProviderCredential;
use vcp_lifecycle::foundation::reconciliation::{ReceiptFetch, ReceiptSource};
use vcp_store::contract::CanonicalStore;

/// Explicit maintenance only: preserve the selected task's original ledger and
/// open it under the canonical revision fence without loading an execution profile.
pub(crate) async fn execute(
    entry: &crate::settings::WorkspaceEntry,
    task: &vcp_domain::TaskId,
    workspace: &std::path::Path,
) -> Result<(u8, serde_json::Value), String> {
    execute_with_source(entry, task, workspace, || {
        let credential =
            ProviderCredential::from_config(crate::credential::require(false)?.expose().to_owned());
        Ok(std::sync::Arc::new(OpenRouterReceipts::new(credential)?))
    })
    .await
}

fn selected<'a>(
    state: impl Into<vcp_store::CurrentStateView<'a>>,
    workspace: &vcp_domain::WorkspaceId,
    id: &vcp_domain::TaskId,
) -> Result<(vcp_domain::task::Task, vcp_domain::accounting::Ledger), String> {
    use vcp_store::contract::Collection;
    let state = state.into();
    let task: vcp_domain::task::Task = state
        .record(Collection::Task, id.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|_| "cost reconciliation task is not in the selected workspace")?;
    if task.parent.is_some()
        || task.root != task.scope.task
        || task.scope.task != *id
        || task.scope.workspace != *workspace
        || task.state != vcp_domain::task::TaskState::Paused
    {
        return Err("cost reconciliation requires a paused root task".into());
    }
    let ledger: vcp_domain::accounting::Ledger = state
        .record(Collection::Ledger, id.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|_| "cost reconciliation requires the original task ledger")?;
    if ledger.scope != task.scope {
        return Err("cost reconciliation ledger scope differs from task".into());
    }
    Ok((task, ledger))
}

async fn execute_with_source(
    entry: &crate::settings::WorkspaceEntry,
    task: &vcp_domain::TaskId,
    workspace: &std::path::Path,
    source: impl FnOnce() -> Result<std::sync::Arc<dyn ReceiptSource>, String>,
) -> Result<(u8, serde_json::Value), String> {
    let mut config = entry.config.clone();
    if entry.rebind_pending || std::path::Path::new(&config.binding.root) != workspace {
        return Err("cost reconciliation workspace binding is unavailable".into());
    }
    let root = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: config.workspace.clone(),
            root: vcp_domain::RootId::parse(config.workspace.as_str())
                .map_err(|e| e.to_string())?,
            repository: config.binding.repository.clone(),
            worktree: config.binding.worktree.clone(),
            binding: config.binding.revision,
        },
        workspace,
    )
    .map_err(|e| e.to_string())?;
    let _root_pin = root.hold(None, true).map_err(|e| e.to_string())?;
    let identity = entry
        .identity
        .as_ref()
        .ok_or("cost reconciliation requires a verified workspace binding")?;
    let _git_pin = if identity.git_directory_identity.is_some() {
        Some(
            root.hold(Some(std::path::Path::new(".git")), true)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    crate::binding::verify(&root, identity)?;
    let store = vcp_store::Store::open(
        &config.canonical_root,
        config.backend,
        &[workspace.to_owned()],
    )
    .await
    .map_err(|e| e.to_string())?;
    let selection = selected(store.current(), &config.workspace, task);
    store.close().await.map_err(|e| e.to_string())?;
    let (task, ledger) = selection?;
    config.session = task.scope.session.clone();
    config.root_task = task.scope.task.clone();
    config.cap = vcp_domain::accounting::MonetaryLimit {
        currency: ledger.currency,
        micros: ledger.cap,
    };
    config.protected = ledger.protected;
    let (host, owner) = vcp_lifecycle::foundation::CanonicalHost::open_selected(
        config.clone(),
        Some(task.revision),
    )?;
    let result = async {
        // Revalidate after recovery under the owner lock, before credentials or
        // metadata transport. Never register a model thread or install a profile.
        selected(
            host.current_state()?.as_ref(),
            &config.workspace,
            &config.root_task,
        )?;
        host.configure_receipt_source(source()?)?;
        let report = host.reconcile_root_pending().await?;
        let snapshot = host.current_state()?;
        let (task, ledger) = selected(snapshot.as_ref(), &config.workspace, &config.root_task)?;
        let mut pending_attempt = false;
        for record in snapshot.records.values().filter(|record| {
            record.workspace == config.workspace
                && record.collection == vcp_store::contract::Collection::Attempt
        }) {
            let attempt: vcp_domain::accounting::Attempt =
                record.decode().map_err(|e| e.to_string())?;
            if attempt.root == config.root_task
                && !matches!(
                    attempt.phase,
                    vcp_domain::accounting::ReservationState::Settled
                        | vcp_domain::accounting::ReservationState::Released
                )
            {
                pending_attempt = true;
            }
        }
        let code = reconciliation_exit(&ledger, pending_attempt);
        Ok((
            code,
            serde_json::json!({
                "kind":"provider_cost_reconciliation", "task":task.scope.task,
                "scope":task.scope, "state":"paused", "ledger":ledger,
                "observations":report.observations, "metadata_only":true, "resumed":false
            }),
        ))
    }
    .await;
    // Close on both success and failure; no model controller was started.
    owner.close().await?;
    result
}

fn reconciliation_exit(ledger: &vcp_domain::accounting::Ledger, pending_attempt: bool) -> u8 {
    if ledger.active == vcp_domain::Micros::ZERO
        && ledger.unresolved == vcp_domain::Micros::ZERO
        && !ledger.overrun
        && !pending_attempt
    {
        0
    } else {
        7
    }
}

#[cfg(test)]
#[path = "provider_reconciliation_tests.rs"]
mod command_tests;

pub(crate) struct OpenRouterReceipts {
    client: reqwest::Client,
    credential: ProviderCredential,
    url: reqwest::Url,
}

impl OpenRouterReceipts {
    pub(crate) fn new(credential: ProviderCredential) -> Result<Self, String> {
        Self::at(credential, "https://openrouter.ai/api/v1/generation")
    }

    #[cfg(feature = "qualification")]
    pub(crate) fn new_qualification(
        credential: ProviderCredential,
        endpoint: &str,
    ) -> Result<Self, String> {
        let url =
            reqwest::Url::parse(endpoint).map_err(|_| "qualified receipt endpoint is invalid")?;
        if credential.header_for_transport() != "synthetic-cli-qualification"
            || url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/v1"
        {
            return Err("receipt qualification requires explicit synthetic credential and literal loopback /v1".into());
        }
        Self::at(credential, &format!("{endpoint}/generation"))
    }

    fn at(credential: ProviderCredential, endpoint: &str) -> Result<Self, String> {
        let key = credential.header_for_transport();
        if key.is_empty() || key.len() > 4096 || key.bytes().any(|c| !c.is_ascii_graphic()) {
            return Err("charge metadata credential is invalid".into());
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| "charge metadata client unavailable")?;
        Ok(Self {
            client,
            credential,
            url: reqwest::Url::parse(endpoint).map_err(|_| "charge metadata endpoint invalid")?,
        })
    }
}

impl ReceiptSource for OpenRouterReceipts {
    fn fetch<'a>(
        &'a self,
        request_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ReceiptFetch, String>> + Send + 'a>> {
        Box::pin(async move {
            if !vcp_models::reconciliation::valid_request_id(request_id) {
                return Err("charge generation identity invalid".into());
            }
            let mut url = self.url.clone();
            url.query_pairs_mut().append_pair("id", request_id);
            let mut response = match self
                .client
                .get(url)
                .bearer_auth(self.credential.header_for_transport())
                .send()
                .await
            {
                Ok(response) => response,
                Err(_) => return Ok(ReceiptFetch::Unavailable),
            };
            if response.status() != reqwest::StatusCode::OK {
                return Ok(ReceiptFetch::Unavailable);
            }
            let mut raw = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "charge metadata read failed")?
            {
                if raw.len().saturating_add(chunk.len())
                    > vcp_models::reconciliation::MAX_RECEIPT_BYTES
                {
                    return Err("charge metadata response too large".into());
                }
                raw.extend_from_slice(&chunk);
            }
            // Credentials supplied back by an untrusted endpoint must not enter
            // canonical evidence. Omit all transport/provider error prose.
            if raw
                .windows(self.credential.header_for_transport().len())
                .any(|w| w == self.credential.header_for_transport().as_bytes())
            {
                return Err("charge metadata contains credential material".into());
            }
            let value: serde_json::Value =
                serde_json::from_slice(&raw).map_err(|_| "charge metadata invalid JSON")?;
            if value
                .to_string()
                .contains(self.credential.header_for_transport())
            {
                return Err("charge metadata contains credential material".into());
            }
            Ok(ReceiptFetch::Found(raw))
        })
    }
}

#[cfg(all(test, feature = "qualification"))]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };
    #[tokio::test]
    async fn metadata_get_never_repeats_inference_and_missing_is_unknown() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/generation"))
            .and(query_param("id", "gen-failed"))
            .and(header(
                "authorization",
                "Bearer synthetic-cli-qualification",
            ))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let source = OpenRouterReceipts::new_qualification(
            ProviderCredential::from_config("synthetic-cli-qualification".into()),
            &format!("{}/v1", server.uri()),
        )
        .unwrap();
        assert!(matches!(
            source.fetch("gen-failed").await.unwrap(),
            ReceiptFetch::Unavailable
        ));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, reqwest::Method::GET);
        assert!(requests[0].body.is_empty());
    }
    #[test]
    fn qualification_never_allows_production_or_redirected_urls() {
        for endpoint in [
            "https://openrouter.ai/api/v1",
            "http://localhost:123/v1",
            "http://127.0.0.1:123/v1?x=y",
            "http://127.0.0.1:123/v1/",
        ] {
            assert!(OpenRouterReceipts::new_qualification(
                ProviderCredential::from_config("synthetic-cli-qualification".into()),
                endpoint
            )
            .is_err());
        }
    }
}
