// SPDX-License-Identifier: Apache-2.0
//! Explicit fixed provider setup probes. Candidate metadata never becomes a qualified
//! Snapshot. The canonical owner still captures, reserves and settles every call.
use super::*;
#[cfg(feature = "qualification")]
pub mod cohort;
#[cfg(feature = "qualification")]
pub mod native;
use vcp_models::{
    catalog::CandidateMetadata,
    request::Tools,
    stream::{Call, ResultBody, Stream},
};

/// The exact amount admission reserves for one request with these bounds,
/// using the worker's own bounds and quote arithmetic. Setup estimates use it
/// to show minimum caps before any provider call.
pub fn request_reservation(
    price: &vcp_domain::accounting::PriceSnapshot,
    reservation_input: Units,
    output: Units,
    now: vcp_domain::Timestamp,
) -> Result<vcp_domain::Micros, String> {
    let bounds = vcp_models::catalog::admission_usage(reservation_input, output)
        .ok_or("reservation bounds overflow")?;
    vcp_budget::arithmetic::quote(price.clone(), bounds, now)
        .map(|quote| quote.amount.micros)
        .map_err(|error| error.to_string())
}

pub const MARKER: &str = "VCP_CONFORMANCE_\u{2603}";
pub const FINAL: &str = "VCP_CONFORMANCE_OK";
pub fn tools() -> serde_json::Value {
    serde_json::json!([{"type":"function","name":"vcp_conformance_echo","description":"Return the exact marker as a local synthetic tool result.","parameters":{"type":"object","properties":{"marker":{"type":"string","enum":[MARKER]}},"required":["marker"],"additionalProperties":false}}])
}
#[derive(Clone)]
pub enum Probe {
    ToolCall,
    Continuation(Call),
}
pub(super) fn body(
    candidate: &CandidateMetadata,
    output: Units,
    probe: &Probe,
) -> Result<serde_json::Value, String> {
    let mut input = vec![
        serde_json::json!({"type":"message","role":"user","content":[{"type":"input_text","text":format!("Call vcp_conformance_echo once with marker {MARKER}. After its result, reply with exactly {FINAL}. Do not call any other tool.")} ]}),
    ];
    if let Probe::Continuation(call) = probe {
        if call.name != "vcp_conformance_echo"
            || call.arguments != serde_json::json!({"marker":MARKER})
            || call.id.is_empty()
            || call.id.len() > 256
        {
            return Err("probe continuation differs from the fixed echo contract".into());
        }
        input.push(serde_json::json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":serde_json::to_string(&call.arguments).map_err(|_|"probe arguments")?}));
        input.push(
            serde_json::json!({"type":"function_call_output","call_id":call.id,"output":MARKER}),
        );
    }
    let price = |category| {
        let rate = &candidate.price.rates[&category];
        if rate.per_units != Units::new(1_000_000) {
            return Err("candidate tariff units".to_owned());
        }
        Ok(format!(
            "{}.{:06}",
            rate.micros.get() / 1_000_000,
            rate.micros.get() % 1_000_000
        ))
    };
    // Request rates use the same million-unit encoding but wire request prices
    // are USD/request rather than USD/million tokens.
    let request_rate = &candidate.price.rates[&ChargeCategory::Request];
    let request = format!(
        "{}.{:012}",
        request_rate.micros.get() / 1_000_000_000_000,
        request_rate.micros.get() % 1_000_000_000_000
    );
    Ok(
        serde_json::json!({"model":candidate.price.model,"input":input,"tools":tools(),"tool_choice":"auto","max_output_tokens":output.get(),"stream":true,"store":false,
        "provider":{"only":[candidate.price.provider],"order":[candidate.price.provider],"allow_fallbacks":false,"require_parameters":true,"data_collection":"deny","zdr":false,
        "max_price":{"prompt":price(ChargeCategory::Input)?,"completion":price(ChargeCategory::Output)?,"request":request}}}),
    )
}
pub struct Lease {
    host: CanonicalHost,
    binding: ThreadBinding,
    attempt: AttemptId,
    body: serde_json::Value,
    parser: Option<Stream>,
    finished: bool,
}
impl Lease {
    pub fn body(&self) -> &serde_json::Value {
        &self.body
    }
    pub fn capture(&mut self, bytes: &[u8]) -> Result<(), String> {
        let attempt = self.attempt.clone();
        let bytes = bytes.to_vec();
        let parsed = bytes.clone();
        self.host
            .worker
            .run(move |context| context.response_chunk(&attempt, &bytes))?;
        self.parser
            .as_mut()
            .ok_or("probe already completed")?
            .push(&parsed)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<ResultBody, String> {
        let normalized = self
            .parser
            .take()
            .ok_or("probe already completed")?
            .finish()
            .map_err(|e| e.to_string())?;
        let binding = self.binding.clone();
        let attempt = self.attempt.clone();
        let result = normalized.clone();
        self.host
            .worker
            .run(move |context| context.complete_conformance(&binding, &attempt, &result))?;
        self.finished = true;
        Ok(normalized)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.finished {
            let binding = self.binding.clone();
            let attempt = self.attempt.clone();
            let _ = self.host.worker.run_cleanup(move |context| {
                context.unknown(
                    &binding,
                    &attempt,
                    "conformance response incomplete; no retry",
                )
            });
        }
    }
}
impl CanonicalHost {
    /// Only fixed public-synthetic probes exist in this feature-gated boundary.
    /// It cannot configure a production provider or publish a compatibility ID.
    pub fn admit_conformance(
        &self,
        binding: ThreadBinding,
        candidate: CandidateMetadata,
        probe: Probe,
    ) -> Result<Lease, String> {
        let admitted = binding.clone();
        let (attempt, body) = self
            .worker
            .run(move |context| context.admit_conformance(&admitted, &candidate, &probe))?;
        Ok(Lease {
            host: self.clone(),
            binding,
            attempt,
            body,
            parser: Some(Stream::new(
                Tools::parse(&tools()).map_err(|e| e.to_string())?,
            )),
            finished: false,
        })
    }
}

#[cfg(test)]
mod reservation_tests {
    use super::*;
    use vcp_domain::{Micros, Timestamp, Units};

    /// Figures recorded for qwen/qwen3.8-max-0902 at alibaba: the setup probe
    /// pair failed under a $0.50 cap, and a 16,384-token task request reserved
    /// $6.492808 (src/evals/release/p8-owner-v3/hidden/provider-budget-analysis.json).
    #[test]
    fn reservations_match_recorded_qwen_admissions() {
        let raw = serde_json::to_vec(&serde_json::json!({"data":{"id":"qwen/qwen3.8-max-0902","endpoints":[{
            "tag":"alibaba","status":0,"context_length":1000000,"max_prompt_tokens":983616,
            "max_completion_tokens":65536,"supported_parameters":["tools","tool_choice","max_tokens"],
            "pricing":{"prompt":"0.000002","completion":"0.000006","input_cache_write":"0.0000025"}}]}}))
        .unwrap();
        let now = Timestamp::new(1_759_400_000_000);
        let candidate = CandidateMetadata::from_endpoints(
            &raw,
            now,
            Timestamp::new(now.get() + 3_600_000),
            "qwen/qwen3.8-max-0902".into(),
            "alibaba".into(),
            "0.001".into(),
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(candidate.max_input, Units::new(983_616));
        let reserve = |output| {
            request_reservation(
                &candidate.price,
                candidate.max_input,
                Units::new(output),
                now,
            )
        };
        assert_eq!(reserve(512), Ok(Micros::new(6_397_576)));
        assert_eq!(reserve(16_384), Ok(Micros::new(6_492_808)));
        assert!(
            request_reservation(&candidate.price, Units::new(u64::MAX), Units::new(1), now)
                .is_err()
        );
        let expired = Timestamp::new(now.get() + 3_600_000);
        assert!(request_reservation(
            &candidate.price,
            candidate.max_input,
            Units::new(512),
            expired
        )
        .is_err());
    }
}
