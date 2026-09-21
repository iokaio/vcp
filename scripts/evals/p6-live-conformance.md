# Capped provider conformance probes

Build the explicit qualification-only executable with the repository's existing
native Cargo runner and `-p vcp-cli --features qualification --bin
vcp-provider-conformance`. Ordinary production builds exclude the executable and
its canonical probe lease. Its optional HTTP dependency is the existing locked
workspace `reqwest`, with redirects, implicit retries and proxies disabled.

The input spec is a private JSON file:

```json
{
  "catalog": "D:/code/Github/vcp/artifacts/p6-catalog-preflight/anthropic--claude-3-haiku.json",
  "catalog_sha256": "<SHA256 of those exact raw endpoint bytes>",
  "model": "anthropic/claude-3-haiku",
  "endpoint": "amazon-bedrock",
  "request_price_limit": "0.001",
  "cap_usd": "2.000000",
  "max_output_tokens": 512,
  "observed_at": "<catalog retrieval UTC Unix milliseconds>",
  "valid_until": "<explicit expiry, no more than one day after observation>"
}
```

These example identifiers are candidates, not quality or availability promises.
The exact endpoint must exist once in the current captured catalog and must not
expand to other listed endpoint variants. Missing required parameters or prices
reject before dispatch. Request prices have an explicit ceiling even when the
catalog omits that category.

```powershell
vcp-provider-conformance C:/private/probe.json C:/private/new-probe-output <authorized-spec-sha256>
```

The coordinator separately accounts for this cap under the shared authorized
trial budget. The executable accepts at most $25 but does not authorize that sum
or coordinate multiple processes. There are at most two sequential requests and
no retries. Each request reserves the conservative full endpoint input/cache
bound in the same canonical root ledger. Only an exact final observed charge
releases that bound. Unknown charges halt the sequence and retain liability.

The first request asks for one static local echo function call. The second returns
the fixed public marker and asks for an exact final text marker. A continuation
must match the first accounted normalized call, not arbitrary caller data. No
model-supplied code, tool command, URL or filesystem operation executes. Each HTTP
request has a 120-second absolute timeout, a 30-second connection timeout, and the
existing bounded Responses parser.

Evidence includes a permanent no-replay claim with spec/catalog/binary/source
identities, original metadata, the canonical task store, attempt/reservation/
settlement records, captured response bytes and a result record. A passing pair
can establish the observed text/tool protocol; it never establishes global
tokenizer behavior. `byte_ceiling_qualified` remains false. If Responses omits an
exact served endpoint, provider policy remains explicitly unqualified. The result
does not automatically create a production Snapshot, profile, role evidence or
model-group membership.

The binary's unit test uses a local scripted HTTP peer, verifies observed charges
different from the reservation tariff, and verifies missing-cost termination. It
makes no paid requests:

```powershell
cargo test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline -p vcp-cli --features qualification --bin vcp-provider-conformance
```
