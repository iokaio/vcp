# P6 bounded decision codecs and routing shadow

`vcp-models::decision` is a pure codec boundary for optional advice. It does not
send requests, select credentials, create reservations, retry, grant authority or
execute an advised action. The caller's deterministic selector remains usable
with `Mode::Disabled` (the default) or `Mode::Deterministic`; `prepare` returns
`None` in both modes. `baseline` labels that local outcome. No model, purpose,
threshold or remote mode is qualified by this module.

## Host integration contract

Build a `Request` from current authorized evidence. Its binding contains scope,
root, step, steering, authority, deletion, policy/catalog digests, a commitment to
the canonical serialized state and evidence IDs with immutable version digests.
The request adds purpose, question revision and deadline. A trusted
`QualifiedEvaluator` names the exact requested/served identities, operation,
purpose, mode, qualification/configuration digests, expiry and provider controls.
Catalog availability alone cannot construct a qualified evaluator.

`prepare(request, policy, now)` returns an opaque `Prepared` with body, endpoint,
request commitment and evaluator. Its commitment includes the entire request,
question definitions, binding, evaluator configuration and mode. Bounds are 16
questions, 32 choice options, 2–10 score levels, 64 evidence references, 64 KiB
canonical request/body and response, a deadline at most 120 seconds away, and at
most eight shared evaluator attempts. These are VCP bounds, not claims about
the service's maximum batch size.

Before send, the host must revalidate current scope, policy, pause, input and
question/evaluator revisions; persist prepared input and its attempt/reservation
linkage; reserve the complete charge ceiling through the canonical root ledger;
and use the existing transport with implicit retry disabled. `attempts_used`
must come from durable host accounting and include retries and evaluator fallback.
This module's preparation never establishes that these steps occurred. Native
Decisions has no documented output-limit request field, so admission cannot
pretend the conventional chat output cap also bounds native usage.

After response, capture raw bytes and settle observed or uncertain usage against
the original attempt before using advice. `decode(prepared, raw, current_binding,
now)` rejects stale bindings/deadlines/qualification, unexpected served identity,
duplicate JSON keys, extra/missing question IDs, invalid types, unlisted choices,
out-of-range numbers and malformed distributions. It validates the whole answer
batch before returning advice. Recheck pause and current question/evaluator policy
when consuming an outcome: `decode` cannot observe host execution state. Shadow
advice must not affect the selected action. Replay uses recorded output and must
not send another request.

The result preserves native yes-probability, ordinal score, option distribution
and provider-reported confidence separately. Probability sums and consistency
checks use a fixed absolute `1e-6` tolerance; response fixtures outside that
tolerance abstain, so a live qualification must establish appropriate precision
before enabling a purpose. Missing optional native fields remain unavailable.
If qualification requires them, missing fields abstain. A Score's level mean is
fractional; transforming it to another scale or threshold requires a separate
versioned consumer policy. Conventional Boolean answers have a separate discrete
variant; no 0/1 native probability is invented. Conventional null is explicit
question abstention.

Usage retains optional token counts and observed cost rounded upward with the
existing USD codec. Missing or invalid cost remains unknown liability, even when
token counts exist. Stale or invalid answers retain separately decoded usage;
unparseable/oversized responses remain wholly uncertain. Host capture must protect
credentials; filtered response evidence must not be presented as unchanged wire
bytes. No decoder result releases a reservation.

## Primary protocol evidence, observed September 20, 2026

This is documentation/schema evidence, not a live-call qualification. No paid
requests or credentials were used for this investigation.

- The public [Jev endpoint catalog](https://openrouter.ai/api/v1/models/typesafe/jev-1.13/endpoints)
  returned `typesafe/jev-1.13`, tag `typesafe`, available status, a served endpoint
  name containing `jev-1.13-20260917`, text-to-decisions modality and 32,000-token
  context. Its price observation was USD 0.042 per million input tokens and zero
  completion price. No price/model is hardcoded as a runtime default.
- OpenRouter's [official Decisions operation](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/funcs/alphaDecisionsCreate.ts)
  specifies `POST https://openrouter.ai/api/alpha/decisions`, JSON and shared
  API-key authentication. The VCP native codec targets exactly this operation.
  That SDK's default automatic retries are not imported.
- Its [request schema](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/decisionsrequest.ts)
  accepts model, state and named questions plus provider preferences. VCP omits
  optional user/session/tracing metadata. The [provider schema](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/providerpreferences.ts)
  includes provider allowlists, disabled fallback, parameter enforcement,
  collection/ZDR controls and price ceilings. Price units differ: prompt/output
  ceilings are USD per million tokens; request ceilings are USD per request.
- Official [Noul](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/decisionsnoulquestion.ts),
  [Choice](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/decisionschoicequestion.ts)
  and [Score](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/decisionsscorequestion.ts)
  definitions establish typed instructions and criteria. The [response schema](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/decisionsresponse.ts)
  exposes answers/model, optional ID/provider, input/output usage and optional cost.
  Choice/Score distributions and confidence are optional in the OpenRouter SDK.
- TypeSafe's [Noul semantics](https://docs.typesafe.ai/primitives/noul),
  [Score semantics](https://docs.typesafe.ai/primitives/score) and
  [confidence definition](https://docs.typesafe.ai/confidence) distinguish a yes
  probability from a level mean and distribution-derived confidence. These explain
  native meaning; they do not prove every optional field reaches OpenRouter.
- OpenRouter also documents [System One compatibility](https://openrouter.ai/docs/guides/community/typesafe-sdk)
  at `/api/v1/systemone`. It accepts bare/prefixed model IDs and reports observed
  model/provider/cost. VCP does not silently switch between this route and alpha;
  route-specific provider-control parity needs qualification first.

The conventional comparator uses OpenRouter Chat Completions with an explicit
closed JSON schema, no tools, no streaming, a 1,024-token output cap and null
abstention. It carries the same provider restrictions. Its mode/identity/purpose
must be separately qualified; it cannot inherit native probability capability
or Jev evidence. Endpoint/schema support, refusals, data-control enforcement and
accounting remain host qualification obligations.

## Canonical routing shadow

`CanonicalHost::evaluate_pending_routing_shadow(thread)` observes an actual
admitted coding attempt. The caller cannot supply a replacement request, route,
quote, evidence list or destination. The worker retains the admitted baseline and
its verified context, then rechecks the original captures, source versions,
authority and destination permissions before reserving a separate helper attempt.
The evaluator must appear in the current routing policy's finite model/provider
allowsets and preserve its data controls. This increment also conservatively
requires autonomous Network/Opaque permission, observes matching host/policy
denials and respects policy time/output ceilings. Missing permission produces a
local baseline outcome, never an implicit grant.

The evaluator is closed model work, following ADR-020 and the decision design's
gateway admission contract. It cannot execute tools. MCP's `Invocation::Remote`
is a remote-tool contract whose opaque-effect approval cannot be used as a model
admission substitute. No tool grant is created or weakened for evaluation; any
downstream tool operation still requires its own ordinary effect admission.
Advice never changes the coding selection. The helper reservation follows the
main reservation and uses the same root ledger and protected verification balance.

The closed transport uses the native Decisions or conventional Chat endpoint.
It has no redirect, implicit retry, fallback, connection pool or tool execution.
Each request has a deadline of at most 120 seconds and bounded request/response
bytes. Owner, task and credential fences govern physical socket I/O. Credentials
are memory-only, independently scoped and filtered from captured responses.
Known charges are recorded even when advice is stale or invalid. Submitted
requests without a trustworthy observed charge conservatively retain unknown
liability, including failures where no complete HTTP body was written; reopening
does not replay them.

The CLI's optional trusted user-profile field is reference-only:

```json
"decisions": {
  "evaluator": {
    "mode": "shadow",
    "qualification": {"artifact": "<installed-artifact-id>", "digest": "<sha256>"}
  },
  "credential_environment": "VCP_DECISION_API_KEY"
}
```

Omission defaults to disabled. `disabled` and `deterministic` perform no evaluator
requests. `advisory` is rejected. A qualification reference names an existing
owner-installed record with scoped catalog and conformance evidence; a catalog
entry or caller-provided hash does not establish qualification. Credentials are
read only from the explicitly named variable during setup, never from profile
tokens, model output or environment enumeration. No production qualification or
paid trial is installed automatically. The owner installation API is separate
from this startup reference; fixture installations cannot enable production I/O.
`ConformanceEvidence::from_record` supplies the typed evidence format only;
constructing that document is not a conformance measurement.
The trusted `install_decision_qualification` host API explicitly installs evidence
and credentials **and enables shadow mode**; it is not a read-only evidence import.

Terminal and headless event loops drive one caller-owned shadow task while
remaining responsive to control commands. Completion, interruption and shutdown
cancel and join it before closing authority. A coding turn can finish before its
shadow comparison; that comparison then supplies no advice, and any uncertain
charge remains visible in canonical history. Library callers must explicitly
drive the asynchronous observer to obtain a comparison.

Production native Decisions remains unavailable with
`native_charge_bound_unqualified`: the retained public protocol evidence does
not establish a finite bound for every billable category. Conventional Chat also
requires independently installed operation/control/price qualification. Synthetic
TLS fixtures prove host mechanics, not service conformance, calibration, utility
or authorization for paid evaluation. P6-04 live qualification and additional
advisory consumers remain separate acceptance gates.

`tests/decision.rs` contains public synthetic fixtures covering disabled/rules
behavior, native wire controls, invalid batches, probability/score consistency,
missing fields, unknown costs, stale responses, duplicate/nonfinite JSON,
attempt bounds and conventional discrete answers. These fixtures do not claim
provider calibration or live transport behavior. No live defaults should be
enabled before the separately capped P6-04 campaign and host lifecycle gates.
