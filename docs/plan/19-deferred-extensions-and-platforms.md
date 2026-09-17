# 19 — Deferred hooks, importers, observers and execution environments

Status: deferred. Owns P10-01 through P10-04. Every item starts after P8-05 and its more specific prerequisites in the ledger. These are independently prioritized extensions, not a reason to postpone the first Windows CLI.

## P10-01 — Hooks

Code organization: `vcp-extensions/hooks/{registry,planner,input,runner,result,receipt}` with engine lifecycle adapters and the existing execution broker. Adapt qualified Gemini G04 registry/planner/runner behavior while preserving VCP authority and durability.

Implement a versioned event contract, deterministic hook order, scoped input/environment, timeout/output limits and recursion bounds. Record input identity before execution and validated output before it affects context or a prepared operation. A hook rewrite changes operation identity and requires fresh policy evaluation; it cannot inherit approval for different arguments.

Tests: before/after lifecycle ordering, malformed output, timeout, crash after external effect, duplicate delivery, recursive trigger, secret environment filtering and rewritten path/arguments. E16/R06 extend to hooks only when this item ships. Unknown effects reconcile through the same journal as other tools.

Exit: hooks can fail without bypassing authority, replaying uncertain effects, hiding cost or wedging the session; supported events/schemas and deliberate upstream differences are documented.

## P10-02 — Configuration import

Code organization: `vcp-extensions/import/{codex,gemini,normalize,preview,apply,compatibility}` with versioned supported-field maps and source provenance. Read import formats as data; never execute imported commands while parsing.

Implement explicit source selection, dry-run mapping and a concrete configuration diff. Separate skills/MCP definitions from executable hooks, credentials and authority. Unsupported fields produce diagnostics; similar field names do not imply semantic compatibility. Preserve the prior VCP configuration and provide rollback.

Tests: supported-version golden examples, unknown future fields, conflicting names/roots, malicious command strings, path escapes, missing secret references and interrupted apply. A imported allow rule cannot widen trusted user/host authority without an explicit configured decision. Do not include secrets in preview history.

Exit: each claimed source/version subset has fixtures and documentation; partial import is visibly partial, and original data remains recoverable.

## P10-03 — Optional observers

Code organization: `vcp-engine/observers/{subscription,debounce,dedup,proposal,budget}` plus small recall/goal/verification observers. Reuse durable event cursors and root task ownership; avoid an independent always-on loop.

Key observer work by relevant task/manifest/diff revision. Bound event queues, frequency, concurrency, step/deadline and billable reservations. Observers propose state changes to the controller and expose activity/cost. Parent pause stops new observer scheduling, and duplicate delivery cannot repeatedly review unchanged work.

Tests: event storms, duplicate revision, slow model, root cap exhaustion, parent pause and stale recommendation after a user correction. Evaluate usefulness against disabled observers using matched tasks and total support cost before enabling defaults.

Exit: measured benefit justifies each observer, and it obeys the same transparency, cost and recovery invariants as required delegation.

## P10-04 — Other execution environments

Code organization: extend `vcp-exec/platform`, `vcp-repository/host_path`, credential/runtime provisioning and packaging adapters; preserve host-independent workspace/task/memory identities. Define explicit capability reports for each host. Implement local Linux/macOS first as prioritized, then WSL/SSH/devcontainer mappings rather than pretending all are one filesystem.

For each environment, implement executable/argument launch, shell and path semantics, case/Unicode/link rules, process-tree ownership, cancellation, filesystem/network restrictions, secrets, PTY and installation. Map local UI host to execution host explicitly. Define encrypted environment transfer and root rebinding; working data remains unencrypted by VCP locally, and cloud-bound backups keep developer-key encryption.

Test the environment on its actual OS/host: process trees, forced disconnect, stale remote effects, permissions, symlink escape, path remapping, container recreation, missing tools and native index compatibility. Rebuild incompatible indexes from retained local data with provenance. An SSH timeout or recreated container cannot justify blindly replaying an external effect.

Exit: each advertised environment has its own supported version/capability matrix, install/upgrade/handoff evidence and declared limitations. Add editor remote support only after these ownership and path contracts are qualified.

## Shared regression requirements

Run relevant E02/E06–E10/E16/E18 and R03/R04/R06/R08 cases, plus the unchanged Windows CLI acceptance suite when shared code changes. Deferred capabilities must not weaken I-01–I-19, introduce hosted VCP services or create alternative canonical state/ledger ownership.
