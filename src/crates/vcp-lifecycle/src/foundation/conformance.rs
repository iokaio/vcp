// SPDX-License-Identifier: Apache-2.0
//! Explicit fixed provider setup probes. Candidate metadata never becomes a qualified
//! Snapshot. The canonical owner still captures, reserves and settles every call.
use super::worker::now;
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

pub const MARKER: &str = "VCP_CONFORMANCE_\u{2603}";
pub const FINAL: &str = "VCP_CONFORMANCE_OK";
pub const CONNECTION_PROMPT: &str =
    "Reply with a short greeting confirming that VCP can reach this model. Do not use tools.";
/// Preview the conservative bound used by isolated setup admission.
pub fn reservation(
    snapshot: &vcp_models::catalog::Snapshot,
    output: Units,
) -> Result<Micros, String> {
    snapshot.current(now()).map_err(|e| e.to_string())?;
    if output == Units::ZERO || output > snapshot.max_output {
        return Err("connection output exceeds provider bounds".into());
    }
    let input = snapshot.max_input;
    let bounds = Usage {
        input: Units::new(input.get().checked_mul(3).ok_or("input bound overflow")?),
        cache_read: input,
        cache_write: input,
        output,
        requests: Units::new(1),
        ..Default::default()
    };
    vcp_budget::arithmetic::quote(snapshot.price.clone(), bounds, now())
        .map(|quote| quote.amount.micros)
        .map_err(|e| e.to_string())
}
pub fn tools() -> serde_json::Value {
    serde_json::json!([{"type":"function","name":"vcp_conformance_echo","description":"Return the exact marker as a local synthetic tool result.","parameters":{"type":"object","properties":{"marker":{"type":"string","enum":[MARKER]}},"required":["marker"],"additionalProperties":false}}])
}
#[derive(Clone)]
pub enum Probe {
    Connection,
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
    let mut body = serde_json::json!({"model":candidate.price.model,"input":input,"tools":tools(),"tool_choice":"auto","max_output_tokens":output.get(),"stream":true,"store":false,
        "provider":{"only":[candidate.price.provider],"order":[candidate.price.provider],"allow_fallbacks":false,"require_parameters":true,"data_collection":"deny","zdr":false,
        "max_price":{"prompt":price(ChargeCategory::Input)?,"completion":price(ChargeCategory::Output)?,"request":request}}});
    if matches!(probe, Probe::Connection) {
        body["input"] = serde_json::json!([{"type":"message","role":"user","content":[{"type":"input_text","text":CONNECTION_PROMPT}]}]);
        if let Some(fields) = body.as_object_mut() {
            fields.remove("tools");
            fields.remove("tool_choice");
        }
    }
    Ok(body)
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
        let tool_schemas = if matches!(probe, Probe::Connection) {
            serde_json::json!([])
        } else {
            tools()
        };
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
                Tools::parse(&tool_schemas).map_err(|e| e.to_string())?,
            )),
            finished: false,
        })
    }
}
