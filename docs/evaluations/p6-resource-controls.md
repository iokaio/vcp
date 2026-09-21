# P6-05 selected resource controls

Status: implementation integrated; native combined verification pending. No live
model qualification or measured profile default is claimed by these controls.

Optional policy input, escalation, retrieval and reasoning fields use the existing
closed preview/apply/rollback transaction. Absent fields are omitted from serialized
policies, preserving legacy canonical bytes and digests. Preview distinguishes
persisted preferences from effective values under current trusted ceilings.

Input selection constrains the host's existing serialized-request byte estimate
at selection, sealed-context validation and admission. It does not add a tokenizer.
Unqualified byte-to-token providers reserve their full declared input capacity.
Escalation selections lower retry, quality-switch and root-attempt maxima or raise
the repeated-failure minimum; missing configured escalation remains unavailable.
Trusted deadlines and decomposition permissions remain authoritative.

Retrieval bounds constrain the explicit memory-query API and cannot enable recall,
expand source access or load an embedding model. They bind the effective policy
across asynchronous querying, materialization and provider send. Reasoning effort
requires both trusted configuration and endpoint-specific capability evidence;
unsupported efforts cannot make a strict pin fall back silently.

Verification recorded in this increment:

- The full model suite passed, including 20 routing tests covering input-bound
  rejection, invalid resource limits and unchanged legacy policy serialization.
- Added canonical-store tests cover component-wise clamping, missing-escalation
  rejection, selected input/escalation apply, cleared preference inheritance and
  rollback under narrowed ceilings: all five policy-edit tests passed on Files
  and SQLite (`artifacts/p6-policy-state-focused.log`). The owner-declaration
  state test also passed (`artifacts/p6-owner-state-focused.log`). CLI tests
  cover explicit values, inheritance, invalid ranges and duplicates: **64 passed,
  one existing test ignored**, recorded in `artifacts/p6-cli-resource-final.log`.
- The broader canonical routing-state suite finished with **40 passed and one
  existing ignored test** (`artifacts/p6-routing-state-final.log`), including
  stored forecasts, retained evidence, policy transactions and declaration fences.
- Native fixtures add input rejection before admission, selected zero quality
  switches, retrieval-policy staleness and actual reasoning request parameters.
  The four-test routing run passed (`artifacts/p6-routing-native-stack16.log`),
  including the owner escalation matrix, selected input/output and effort cases.
  The retrieval test also passed (`artifacts/p6-retrieval-native-rerun.log`),
  including the two-token query cap and a routing policy introduced after fixed
  context preparation. Both runs cover Files and SQLite. The first retrieval
  fixture incorrectly tried to prepare a fixed context after enabling automatic
  routing; it was corrected to exercise the supported preparation path.
  The routing run uses the repository's documented 16 MiB test stack; its initial
  default-stack invocation overflowed in the existing decision-shadow test.
  Earlier output-limit checks are recorded in [output-limit evidence](p6-output-limits.md).
- The earlier fixed-provider startup fixture passed on Files and SQLite:
  `output_tokens=512`, `max_transport_retries=0` produced a 512-token body and
  reservation, one attempt after HTTP 503, and retained unresolved liability.
  Legacy configuration defaults to two retries; startup rejects three before
  creating the store. Logs: `artifacts/p6-startup-zero-retry.log` and
  `artifacts/p6-startup-legacy-config.log`.

Concurrency control needs P7's actual child scheduler and stays explicitly pending.
These deterministic fixtures provide integration evidence, not task-quality or
cost-improvement evidence for live models.
