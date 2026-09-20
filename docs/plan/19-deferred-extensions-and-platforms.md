# 19 — Deferred hooks, importers, observers and execution environments

Status: deferred. Owns P10-01 through P10-04. Every item starts after P8-05 and its more specific prerequisites in the ledger. These are independently prioritized extensions, not a reason to postpone the first Windows CLI.

The proposed [extension design](../architecture/routing-extensions-design.md#deferred-hook-import-and-observer-contracts)
and [execution-host design](../architecture/deferred-clients-design.md#execution-host-contract)
provide detailed contracts. Consult [extension ADR-011](../adr/011-extension-scope.md),
[compatibility ADR-014](../adr/014-foreign-compatibility.md) and
[client/distribution ADR-012](../adr/012-clients-and-distribution.md) before declaring
new compatibility. These designs do not select upstream revisions, enable features
or supply operating-system evidence.

## P10-01 — Hooks

Code organization: `vcp-extensions/hooks/{registry,planner,input,runner,result,receipt}` with engine lifecycle adapters and the existing execution broker. Adapt qualified Gemini G04 registry/planner/runner behavior while preserving VCP authority and durability.

Implement a versioned event contract, deterministic hook order, scoped input/environment, timeout/output limits and recursion bounds. Record input identity before execution and validated output before it affects context or a prepared operation. A hook rewrite changes operation identity and requires fresh policy evaluation; it cannot inherit approval for different arguments.

Tests: before/after lifecycle ordering, malformed output, timeout, crash after external effect, duplicate delivery, recursive trigger, secret environment filtering and rewritten path/arguments. E16/R06 extend to hooks only when this item ships. Unknown effects reconcile through the same journal as other tools.

**Construction sequence.** Define the hook event/input/output schemas for the
reserved points in [architecture section 15.3](../architecture/vcp-what.md#153-hook-lifecycle).
Record hook ID/version/source hash, event and input identity, workspace/root/task,
causation/depth, permitted artifact refs, effect scope, timeout/output bounds and
failure policy. Define deterministic order from configured priority and identity;
reject ambiguous/cyclic ordering rather than depending on filesystem enumeration.
Build a pure planner so ordering and deduplication are testable without execution.

Execute only through the existing broker with restricted environment and granted
scope. Persist the planned input and dispatch intent before launch, and validate
output before publishing its result. Context proposals remain attributed input;
argument rewrites create a new operation identity, revalidate schemas/resource
versions and return to policy evaluation. An approval for the old command cannot
authorize a different path or shell fragment. Hooks cannot directly write memory
acceptance or budget balances.

Deduplicate by hook/event/input identity and bound trigger depth/fan-out. A restarted
hook with unknown external effects stays in reconciliation, not an automatic rerun.
Define security/validation failure as blocking the affected action and optional
notification failure as a visible warning; a timeout must not accidentally become
allow. Parent pause blocks new hook dispatch while the client remains available for
inspection. A completed hook output arriving after steering must be revalidated.

Retain qualified G04 upstream fixtures separately from VCP expectations and document
intentional differences under ADR-014. Use an external marker plus kill barriers
after intent, external effect and before result commit; test duplicate lifecycle
delivery and a rewrite after approval. Assert effect count, fresh authorization and
bounded recursion, not merely runner exit status. Keep executable hook support
independent of first-release skill discovery.

Exit: hooks can fail without bypassing authority, replaying uncertain effects, hiding cost or wedging the session; supported events/schemas and deliberate upstream differences are documented.

## P10-02 — Configuration import

Code organization: `vcp-extensions/import/{codex,gemini,normalize,preview,apply,compatibility}` with versioned supported-field maps and source provenance. Read import formats as data; never execute imported commands while parsing.

Implement explicit source selection, dry-run mapping and a concrete configuration diff. Separate skills/MCP definitions from executable hooks, credentials and authority. Unsupported fields produce diagnostics; similar field names do not imply semantic compatibility. Preserve the prior VCP configuration and provide rollback.

Tests: supported-version golden examples, unknown future fields, conflicting names/roots, malicious command strings, path escapes, missing secret references and interrupted apply. An imported allow rule cannot widen trusted user/host authority without an explicit configured decision. Do not include secrets in preview history.

**Construction sequence.** Snapshot the explicitly selected source configuration
and record format/version/hash and allowed root. Parse under file/byte/depth limits
with no shell interpolation, include execution or automatic remote resolution.
Normalize into a proposed VCP configuration object through a versioned field map;
every source field receives a mapped, ignored-with-reason, unsupported or conflicting
status. Unknown future versions cannot silently use the latest-known mapping.

Produce a preview with concrete old/new values, source provenance, unsupported
semantics, credential-reference gaps and executable/authority consequences. Redact
secret-bearing source values before any durable history capture; retain only safe
field/location diagnostics or explicitly protected references. Similar names such
as `allow`, `model` or `cwd` are not proof of semantic equivalence. Imported skills,
MCP definitions and hooks retain their separate activation/trust paths. Any command
strings remain inert data until their later authorized use.

Apply only selected fields under expected VCP configuration revision and trusted
ceilings. Use a recoverable write/publication transaction and keep the prior revision
for rollback. A concurrently changed VCP config requires a refreshed preview.
Rollback creates a current legal revision; it cannot resurrect revoked grants or
copy old secrets into public history. Missing credential references remain actionable
setup state, never an invitation to discover ambient provider accounts.

For each advertised source/version subset, retain small synthetic golden fixtures
and expected diagnostics, including unsupported hooks and malicious includes/path
escapes. Crash before/after publication and prove either the previous or new complete
configuration is selected. Use [architecture section 15.5](../architecture/vcp-what.md#155-gemini-derived-hooks-and-explicit-compatibility)
and [import design](../architecture/routing-extensions-design.md#deferred-hook-import-and-observer-contracts).
Publish the supported-field matrix and explicit exclusions with the importer.

Exit: each claimed source/version subset has fixtures and documentation; partial import is visibly partial, and original data remains recoverable.

## P10-03 — Optional observers

The [M10 regime-filter observer](21-markov-integration.md#m8m10--reuse-and-qualification-campaigns)
is a planned candidate here after P8-05/P7-06, with no first-release dependency.
Reuse qualified local stall filtering, durable cursors and revision-bound proposals;
measure benefit against disabled observers and display inferred regimes as
estimates. Keep scheduling bounded and stopped by parent pause, including when
evaluation uses local arithmetic rather than a provider.

Code organization: `vcp-engine/observers/{subscription,debounce,dedup,proposal,budget}` plus small recall/goal/verification observers. Reuse durable event cursors and root task ownership; avoid an independent always-on loop.

Key observer work by relevant task/manifest/diff revision. Bound event queues, frequency, concurrency, step/deadline and billable reservations. Observers propose state changes to the controller and expose activity/cost. Parent pause stops new observer scheduling, and duplicate delivery cannot repeatedly review unchanged work.

Tests: event storms, duplicate revision, slow model, root cap exhaustion, parent pause and stale recommendation after a user correction. Evaluate usefulness against disabled observers using matched tasks and total support cost before enabling defaults.

**Construction sequence.** Define each observer's trigger filter, required input
revision, output proposal type and expected measurable benefit before writing a
generic background loop. Maintain a durable cursor and deduplication key containing
observer version, root/task, trigger class and relevant manifest/diff revision.
Debounce/coalesce events before spend admission; cap queue size, outstanding work,
calls, concurrency and deadline. Repeated delivery for unchanged input reuses the
existing proposal or pending attempt rather than generating another charge.

Create a bounded observation task under the root, assemble only authorized context
and admit any model work through OpenRouter and the shared ledger. Display active
observer work and cost like other support roles. Local deterministic observers need
no artificial model call. Persist inputs and outstanding attempts so restart can
reconcile them; a late result for an old diff remains historical advice and cannot
change the new task revision.

Route recommendations through the controller's normal proposal validation and
authority checks. Observers do not grant permissions, alter caps or accept memory
directly. `/pause` stops fresh observer scheduling while status remains readable;
owner loss invokes the same barrier. Resume is explicit and checks whether queued
input is still relevant. Cancellation preserves uncertain model liabilities.

Use matched disabled/enabled task runs with predeclared grading, full main/helper
cost, interventions, duplicate-call counts and latency. Report negative or uncertain
benefit rather than enabling an observer because it emits plausible prose. Apply
[architecture section 16.5](../architecture/vcp-what.md#165-observers) and
[observer contracts](../architecture/routing-extensions-design.md#deferred-hook-import-and-observer-contracts).
Qualification of an optional observer cannot become a prerequisite for required
delegation or the first release.

Exit: measured benefit justifies each observer, and it obeys the same transparency, cost and recovery invariants as required delegation.

## P10-04 — Other execution environments

Code organization: extend `vcp-exec/platform`, `vcp-repository/host_path`, credential/runtime provisioning and packaging adapters; preserve host-independent workspace/task/memory identities. Define explicit capability reports for each host. Implement local Linux/macOS first as prioritized, then WSL/SSH/devcontainer mappings rather than pretending all are one filesystem.

For each environment, implement executable/argument launch, shell and path semantics, case/Unicode/link rules, process-tree ownership, cancellation, filesystem/network restrictions, secrets, PTY and installation. Map local UI host to execution host explicitly. Define encrypted environment transfer and root rebinding; working data remains unencrypted by VCP locally, and cloud-bound backups keep developer-key encryption.

Test the environment on its actual OS/host: process trees, forced disconnect, stale remote effects, permissions, symlink escape, path remapping, container recreation, missing tools and native index compatibility. Rebuild incompatible indexes from retained local data with provenance. An SSH timeout or recreated container cannot justify blindly replaying an external effect.

**Construction sequence.** For each separately prioritized host, first write a
capability matrix covering launch/shell, path/case/Unicode/link handling, ownership,
process-tree termination, filesystem/network enforcement, PTY, secrets, inference,
indexes and installation. Mark controls as qualified/unsupported/unknown with
evidence references; required but unavailable enforcement rejects dispatch. A POSIX
adapter name is not a claim that Linux, macOS, WSL and containers share guarantees.

Represent resources by host/workspace/root identity and relative path until resolved
on the execution host. Keep UI host and canonical-store owner explicit. Qualify
root rebinding, mount/drive boundaries, case collisions, link escapes and container
identity changes; do not send local canonical paths to another host as equivalent
resources. Destination authority and credential references are configured there,
not inherited wholesale from the UI environment. Use the common broker API for
actual execution and durable intent/receipt identity across transport failure.

Implement liveness/ownership so loss of the controlling task owner stops new
root/child work and attempts bounded cancellation on the actual host. Disconnect
cannot certify termination: query a durable authenticated remote operation identity
or record unknown effects. A host incapable of the required owner-loss behavior
must expose the limitation and remain unqualified for that claim. Live pause must
work without closing the controlling client, with deliberate resume after
reconciliation.

Transfer history and liabilities through the established encrypted snapshot/handoff
path, keeping recovery keys outside cloud destinations and active local data
plaintext. Validate host/schema/model/tokenizer/native-index compatibility before
reuse; rebuild only derived indexes from retained canonical inputs when needed.
Do not turn a failed native index open into silently empty memory. Packaging must
include the actual host dependencies and applicable notices, with install/upgrade
tests outside the development checkout.

Use [execution-host design](../architecture/deferred-clients-design.md#execution-host-contract),
[platform capability requirements](../architecture/vcp-what.md#103-platform-capability-matrix)
and [pause/recovery requirements](../architecture/vcp-what.md#45-terminal-close-pause-and-workspace-resume).
Maintain one evidence row per advertised OS/host/version and workload, including
actual process/filesystem observations, disconnect/kill results and not-run cases.
Native Windows remains a separate regression gate; WSL success cannot replace it.

Exit: each advertised environment has its own supported version/capability matrix, install/upgrade/handoff evidence and declared limitations. Add editor remote support only after these ownership and path contracts are qualified.

## Shared regression requirements

Run relevant E02/E06–E10/E16/E18 and R03/R04/R06/R08 cases, plus the unchanged Windows CLI acceptance suite when shared code changes. Deferred capabilities must not weaken I-01–I-19, introduce hosted VCP services or create alternative canonical state/ledger ownership.
