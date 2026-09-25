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
  "max_output_tokens": 2048,
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
or coordinate multiple processes. The requested output ceiling must be 1–2,048
tokens and remains bounded by the selected endpoint. Historical probe receipts
retain the limit actually used; raising this ceiling does not authorize replay.
There are at most two sequential requests and no retries. Each request reserves
the conservative full endpoint input/cache bound in the same canonical root ledger. Only an exact final observed charge
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

After a completed pair, `--qualify <sources.json> <new-output> <sources-sha256>`
performs an offline evidence join and writes `snapshot.json`. Each of
`probe_spec`, `report`, `catalog`, and the two `generations` entries has `path`
and `sha256`. `observed_at` and `valid_until` are decimal millisecond strings.
Generation records must come from the authenticated read-only OpenRouter
`/api/v1/generation?id=<observed-response-id>` endpoint. Capture a fresh complete
model catalog; capability, context/output and normalized tariff drift rejects.
The explicit source hash is the review boundary; this command performs no network
operation and does not authenticate arbitrary user-supplied JSON itself.

[ADR-041](../../docs/adr/041-provider-evidence-and-conservative-routing.md)
defines the strict receipt/catalog join. Both requests must have one successful
provider attempt, matching IDs/charges, a stable observed model revision and
internal endpoint UUID, and an unambiguous provider name in the complete catalog.
The raw Responses identity remains unchanged. The snapshot can establish dated
provider-policy/text-tool compatibility but retains `byte_ceiling_qualified:false`;
automatic routing uses the separately verified full-input reservation fallback.
Quality/group membership is still independent measured evidence.
