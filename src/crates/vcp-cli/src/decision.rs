// SPDX-License-Identifier: Apache-2.0
//! Explicit shadow configuration and caller-owned asynchronous observation.
use serde::Deserialize;
use vcp_lifecycle::foundation::{decision, CanonicalHost};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub evaluator: decision::Configuration,
    pub credential_environment: Option<String>,
    pub local_fit: Option<decision::EvidencePin>,
}
impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        self.evaluator.validate()?;
        if self
            .local_fit
            .as_ref()
            .is_some_and(|pin| !vcp_domain::accounting::valid_hash(&pin.digest))
        {
            return Err("invalid local fit evidence pin".into());
        }
        if self.credential_environment.as_ref().is_some_and(|name| {
            name.is_empty()
                || name.len() > 128
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }) {
            return Err("invalid decision credential environment variable name".into());
        }
        Ok(())
    }
    pub fn install(&self, host: &CanonicalHost) -> Result<(), String> {
        self.validate()?;
        host.configure_decisions(self.evaluator.clone())?;
        if self.evaluator.mode == decision::Mode::Shadow {
            if let Some(pin) = &self.local_fit {
                host.select_local_shadow(pin.clone())?;
            }
        }
        if self.evaluator.mode == decision::Mode::Shadow && self.evaluator.qualification.is_some() {
            if let Some(name) = &self.credential_environment {
                let material = decision::CredentialMaterial::bearer(
                    std::env::var(name)
                        .map_err(|_| "configured decision credential is unavailable")?,
                )
                .map_err(|_| "configured decision credential format rejected")?;
                host.install_decision_credential(material)?;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Driver {
    running: Option<tokio::task::JoinHandle<Result<decision::ShadowOutcome, String>>>,
}
impl Driver {
    pub fn active(&self) -> bool {
        self.running.is_some()
    }

    /// No network await occurs on the command/event pump. Only a finished task
    /// is joined here; its durable result remains available through inspection.
    pub async fn poll(
        &mut self,
        host: &CanonicalHost,
        thread: codex_protocol::ThreadId,
    ) -> Option<&'static str> {
        let mut notice = None;
        if self.running.as_ref().is_some_and(|task| task.is_finished()) {
            if let Some(task) = self.running.take() {
                notice = Some(match task.await {
                    Ok(Ok(outcome)) if outcome.evaluator_attempt.is_none() && outcome.artifacts.is_empty() =>
                        "Decision shadow skipped; no evaluator request admitted. Coding selection unchanged.",
                    Ok(Ok(_)) => "Decision shadow recorded; coding selection unchanged. Inspect history for advice and cost.",
                    _ => "Decision shadow unavailable; inspect history for retained costs and evidence.",
                });
            }
        }
        if self.running.is_none() && host.decision_shadow_pending(thread).unwrap_or(false) {
            let host = host.clone();
            self.running = Some(tokio::spawn(async move {
                host.evaluate_pending_decision_shadow(thread).await
            }));
        }
        notice
    }

    /// Join cancellation before owner shutdown or resume: dropping a handle
    /// alone would leave the host's durable settlement guard still running.
    pub async fn cancel(&mut self) {
        if let Some(task) = self.running.take() {
            task.abort();
            let _ = task.await;
        }
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        if let Some(task) = &self.running {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_no_raw_credential_or_advisory_escape() {
        let disabled: Configuration = serde_json::from_value(serde_json::json!({
            "evaluator": {"mode":"disabled","qualification":null}
        }))
        .unwrap();
        assert!(disabled.validate().is_ok());
        assert!(serde_json::from_value::<Configuration>(serde_json::json!({
            "evaluator": {"mode":"shadow","qualification":null}, "credential":"secret"
        }))
        .is_err());
        let invalid = Configuration {
            evaluator: decision::Configuration::default(),
            credential_environment: Some("KEY=embedded-value".into()),
            local_fit: None,
        };
        assert!(invalid.validate().is_err());
        let advisory = Configuration {
            evaluator: decision::Configuration {
                mode: decision::Mode::Advisory,
                qualification: None,
            },
            credential_environment: None,
            local_fit: None,
        };
        assert!(advisory.validate().is_err());
        let local: Configuration = serde_json::from_value(serde_json::json!({
            "evaluator": {"mode":"shadow","qualification":null},
            "local_fit": {"artifact": "local-fit-fixture", "digest": "a".repeat(64)}
        }))
        .unwrap();
        assert!(local.validate().is_ok());
        let mut invalid_local = local;
        invalid_local.local_fit.as_mut().unwrap().digest = "unverified".into();
        assert!(invalid_local.validate().is_err());
    }

    #[tokio::test]
    async fn cancellation_joins_settlement_guard_before_returning() {
        let (entered, ready) = tokio::sync::oneshot::channel();
        let settled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        struct Guard(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let observed = settled.clone();
        let mut driver = Driver {
            running: Some(tokio::spawn(async move {
                let _guard = Guard(observed);
                let _ = entered.send(());
                std::future::pending::<Result<decision::ShadowOutcome, String>>().await
            })),
        };
        ready.await.unwrap();
        driver.cancel().await;
        assert!(!driver.active());
        assert!(settled.load(std::sync::atomic::Ordering::SeqCst));
    }
}
