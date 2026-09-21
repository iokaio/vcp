// SPDX-License-Identifier: Apache-2.0
//! One fixed native mixed-batch bootstrap. This never installs qualification.
use super::*;
use vcp_models::{
    catalog::CandidateMetadata,
    decision::{self, native_bound::NativeChargeBound},
};
use vcp_protocol::canonical_bytes;

pub fn candidate(
    raw: &[u8],
    model: &str,
    provider: &str,
    request_cap: &str,
    expires: Timestamp,
) -> Result<CandidateMetadata, String> {
    let bound = NativeChargeBound::from_endpoints(raw, model, provider, request_cap)
        .map_err(|e| e.to_string())?;
    let zero = vcp_domain::accounting::Rate {
        micros: Micros::ZERO,
        per_units: Units::new(1_000_000),
    };
    let mut price = PriceSnapshot {
        id: String::new(),
        capability: bound.raw_sha256.clone(),
        model: model.into(),
        provider: provider.into(),
        currency: "USD".to_owned().try_into().map_err(|_| "currency")?,
        valid_until: expires,
        rates: std::collections::BTreeMap::from([
            (ChargeCategory::Input, bound.input_rate),
            (ChargeCategory::CacheRead, bound.cache_read_rate),
            (ChargeCategory::CacheWrite, bound.cache_write_rate),
            (ChargeCategory::Output, zero.clone()),
            (ChargeCategory::ProviderTool, zero),
            (ChargeCategory::Request, bound.request_rate),
        ]),
    };
    price.id = vcp_protocol::digest_bytes(&canonical_bytes(&price).map_err(|e| e.to_string())?);
    Ok(CandidateMetadata {
        raw_sha256: bound.raw_sha256,
        context: bound.input,
        max_input: bound.input,
        max_output: Units::new(1),
        price,
    })
}
pub(in crate::foundation) fn body(
    candidate: &CandidateMetadata,
) -> Result<serde_json::Value, String> {
    let rate = |category| -> Result<String, String> {
        let r = &candidate.price.rates[&category];
        if r.per_units != Units::new(1_000_000) {
            return Err("native tariff units".into());
        }
        Ok(format!(
            "{}.{:06}",
            r.micros.get() / 1_000_000,
            r.micros.get() % 1_000_000
        ))
    };
    let request = &candidate.price.rates[&ChargeCategory::Request];
    let request = format!(
        "{}.{:012}",
        request.micros.get() / 1_000_000_000_000,
        request.micros.get() % 1_000_000_000_000
    );
    Ok(serde_json::json!({"model":candidate.price.model,
        "state":"Public synthetic fixture: the reported check failed three times with identical errors and no code changes. An independent review has not yet run.",
        "questions":{
            "repeated":{"type":"noul","instructions":"Does the supplied state describe repeated failure without observed progress?"},
            "action":{"type":"choice","instructions":"Choose the best next step from the supplied state.","criteria":{"review":"Obtain independent evidence before trying the same strategy again.","repeat":"Retry the unchanged strategy immediately."}},
            "severity":{"type":"score","instructions":"Rate the need for additional evidence.","criteria":["No need","Some need","Strong need"]}},
        "provider":{"only":[candidate.price.provider],"order":[candidate.price.provider],"allow_fallbacks":false,"require_parameters":true,"data_collection":"deny","zdr":false,"max_price":{"prompt":rate(ChargeCategory::Input)?,"completion":"0","request":request}}}))
}
pub struct NativeLease {
    pub(super) host: CanonicalHost,
    pub(super) binding: ThreadBinding,
    pub(super) attempt: AttemptId,
    pub(super) body: serde_json::Value,
    pub(super) bytes: Vec<u8>,
    pub(super) finished: bool,
    pub(super) operation: decision::Operation,
}
impl NativeLease {
    pub fn body(&self) -> &serde_json::Value {
        &self.body
    }
    pub fn capture(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.bytes.len().saturating_add(bytes.len()) > decision::MAX_BYTES {
            return Err("native response byte limit".into());
        }
        let attempt = self.attempt.clone();
        let bytes = bytes.to_vec();
        let retained = bytes.clone();
        self.host
            .worker
            .run(move |c| c.response_chunk(&attempt, &bytes))?;
        self.bytes.extend_from_slice(&retained);
        Ok(())
    }
    pub fn finish(mut self) -> Result<serde_json::Value, String> {
        let (raw, usage) =
            decision::observed_usage(&self.bytes, self.operation).map_err(|e| e.to_string())?;
        let binding = self.binding.clone();
        let attempt = self.attempt.clone();
        let parsed = raw.clone();
        self.host.worker.run(move |c| {
            c.complete_native_conformance(&binding, &attempt, &parsed, usage.observed_cost)
        })?;
        self.finished = true;
        Ok(raw)
    }
}
impl Drop for NativeLease {
    fn drop(&mut self) {
        if !self.finished {
            let binding = self.binding.clone();
            let attempt = self.attempt.clone();
            let _ = self.host.worker.run_cleanup(move |c| {
                c.unknown(&binding, &attempt, "native probe incomplete; no retry")
            });
        }
    }
}
impl CanonicalHost {
    pub fn admit_native_conformance(
        &self,
        binding: ThreadBinding,
        raw: Vec<u8>,
        model: String,
        provider: String,
        request_cap: String,
        expires: Timestamp,
    ) -> Result<NativeLease, String> {
        let admitted = binding.clone();
        let (attempt, body) = self.worker.run(move |c| {
            c.admit_native_conformance(&admitted, &raw, &model, &provider, &request_cap, expires)
        })?;
        Ok(NativeLease {
            host: self.clone(),
            binding,
            attempt,
            body,
            bytes: Vec::new(),
            finished: false,
            operation: decision::Operation::JevDecisions,
        })
    }
}
