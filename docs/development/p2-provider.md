# P2 OpenRouter gateway increment

The canonical host now consumes [captured context seals](p2-context.md) before
actual retained HTTP requests. `vcp-models` owns provider conversion and bounded
normalization; the existing Codex transport and controller remain in use.
P2-02 is in progress. Scripted transport qualification does not establish live
provider compatibility, and the automatic coding driver remains P2-05.

## Provider contract

The implementation uses the OpenRouter Responses endpoint and full stateless
conversation inputs. Its explicit transport profile sets the HTTPS origin,
accepts a typed credential, disables implicit credentials and retries, and turns
off the upstream instruction loader. VCP's scoped AGENTS.md observations supply
the sealed prompt. Provider headers do not enter request/response capture.

`vcp-models/src/catalog.rs` accepts bounded endpoint metadata with a dated,
explicit compatibility record. It retains the raw hash, context/output limits,
supported parameters and checked decimal rates. Missing token prices reject.
A base provider slug that includes known regional/variant endpoints rejects;
this codec admits one qualified endpoint, not an unqualified provider pool.
A missing separate prompt bound uses the total context window; input plus output
and margin must still fit. Request pricing is bounded by an explicit configured
USD/request ceiling, which is also sent as `provider.max_price.request`.
An omitted request price is never silently treated as free. Prompt/completion
price ceilings preserve the admitted snapshot against a routing-time price change.
Caching admission conservatively uses at least the ordinary input rate.

`request.rs` supports text messages and local function tools with a bounded JSON
schema subset: typed objects with explicit properties/no additional keys,
required keys, arrays, scalar types and enums. Unknown schema keywords, remote
tools, multimodal content and hidden provider plugins are incompatible. Logical
trust, source ranges and scoped instruction applicability remain attributed in
the conversion. Tool arguments must match a registered schema.

Provider order/allowlist, disabled fallback, required parameters, data-collection
restriction, optional ZDR and price ceilings are explicit serialized fields.
Current compatibility still needs separate qualification for a real selected
model/endpoint, including tokenizer estimation and provider-policy enforcement.
The UTF-8 byte ceiling is a conservative text estimate, not an exact token count.

## Admission and observation

`CanonicalHost::configure_provider` verifies the catalog-derived snapshot and
records configuration before enabling the path. A reopened store with this marker
requires explicit provider reconfiguration. `prepare_context` resolves selected
bytes through current canonical artifact access. Every retained attempt consumes
one prepared seal. Immediately before reservation the worker checks task/steering,
authority/deletion/binding, schemas, source versions and instruction probes,
captures the manifest, and checks again. The exact converted body is captured
before the shared ledger commits admission/send intent. Helpers use the same
boundary; an unprepared helper cannot send.

The native root APIs grant outside parent roots only AGENTS.md applicability.
Canonical skills/memory revisions remain zero in this scaffold; their later
adapters must explicitly integrate their versions. A model body cannot promote
itself into any of those capabilities.

`stream.rs` bounds transport chunks, frame bytes, aggregate bytes, events and
function arguments. It handles arbitrary UTF-8/JSON/CRLF splits and interleaved
call IDs. Partial arguments remain observations. Completed calls are exposed only
after a qualified terminal agrees with the observed fragments. Identical duplicate
terminal events do not create a second result; conflicting events fail.
Unknown served identities remain unknown. Usage is cumulative, cache/reasoning
counts are subsets, and settlement uses observed cost rather than invented cost
from a requested-model assumption. Missing cost retains liability. Raw terminal
usage can reconcile even if cancellation wins the retained completion callback.

One absolute deadline covers response headers and streamed body, so keepalives
cannot extend it. The default is 120 seconds; callers may select a shorter bound.
The explicit transport profile also has a 30-second idle limit. Timeouts after
submission retain liability. The pure retry policy classifies failures and bounds
delay/count/deadline; automatic rescheduling/reassembly belongs to P2-05. The
current profile performs zero implicit retries and consumes a new seal for each
deliberately prepared attempt.

The retained client captures through its terminal boundary. It does not claim
observation of unseen trailing network bytes. Codec rejection preserves captured
evidence and fences continuation. A response from a different observed model is
accounted for and pauses the task.

## Primary compatibility inputs

Reviewed September 18, 2026:

- [Responses overview](https://openrouter.ai/docs/api_reference/responses/overview): stateless endpoint and authentication.
- [Responses schema](https://openrouter.ai/docs/api/api-reference/responses/create-a-response): request fields, provider preferences, price ceilings and usage cost.
- [Function tools](https://openrouter.ai/docs/api_reference/responses/tool-calling): function/call/result identities and completed arguments.
- [Provider routing](https://openrouter.ai/docs/guides/routing/provider-selection): provider restrictions, fallback and data policy.
- [Endpoint metadata](https://openrouter.ai/docs/api/api-reference/endpoints/list-all-endpoints-for-a-model): endpoint capabilities and pricing.

Public metadata was inspected without credentials or model calls. Documentation
examples are not imported as production compatibility fixtures; deterministic
tests use original synthetic records. No real model/price support is promised
without the separately capped live smoke gate.

## Reproduction

```powershell
pwsh -NoProfile -File scripts/test-provider.ps1
pwsh -NoProfile -File scripts/test-integration.ps1
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The provider runner records 16 codec/context tests and the retained absolute
deadline regression. Integration records native host tests, containment and the
private scripted CLI. These require the documented native toolchain; no private
credentials or paid calls are needed. Missing prerequisites are `not_run`.
See [the qualification record](../evaluations/p2-provider-increment.md) for actual
executed evidence and remaining acceptance.
