# P2 authority and durable approvals

P2-03 is in progress. The original `vcp-policy` package evaluates immutable
operations; the canonical engine records policies, grants and user decisions in
the existing store. These APIs are trusted host/library interfaces. A serialized
operation, an `Allow` decision or a grant record is not an executable capability.
P2-04 must connect the native broker and prove actual dispatch counts before
P2-03 acceptance is complete. Retained model-requested tools remain denied by
the canonical host pending that adapter.

## Pure decisions

`vcp-domain/src/policy.rs` defines operation, resource, invocation, isolation,
preset, denial and grant data. `vcp-policy::Prepared` validates bounded canonical
arguments and seals the complete operation identity. Its hash binds workspace,
session/task, actor, host/root binding, authority, steering/policy revisions,
tool/schema, native resource observations, invocation and resource limits.
Process identity includes executable identity, exact argument vector, explicit
shell choice, directory and filtered-environment digest. Shell/process and
remote invocations carry conservative opaque effects; matching shell text alone
cannot grant execution.

The evaluator checks owner/task availability, current scope/revisions/source
facts, trusted workspace, resource limits and actual platform capabilities.
Trusted host denials are supplied separately from user policy and run before
grants or preset rules. Repository text, model output and schema descriptions
have no authority input channel. Money is absent from this API; model admission
continues to use its independent canonical ledger revision.

| Preset | Implemented decision |
|---|---|
| `plan` | Local reads in configured roots; mutation/process requests deny even with a grant |
| `ask` | Local reads in configured roots; other operations use valid grants or ask |
| `workspace` | Initial default; local reads/edits in configured roots; processes require an explicit configured or exact grant |
| `autonomous` | Only explicitly configured automatic effect classes in configured roots, subject to all other checks |

Grants bind actor, workspace/session/task scope, host/binding, authority, policy
and expiry. Exact grants bind the entire prepared digest. Configured grants
match tool/schema, canonical arguments, invocation and isolation exactly, with
explicit root/path prefixes and time/output ceilings. A path prefix observes
component boundaries, so `src` cannot authorize `src-escape`. ASCII comparison
is case-insensitive. Unicode grant paths require exact spelling; ambiguous
Unicode denial comparisons conservatively deny the matching root. Native path
identity and junction enforcement remain the broker's responsibility.

`Facts` cannot be deserialized. The trusted host must supply fresh native source
observations and supported isolation; user approval cannot create an unavailable
OS guarantee. The pure library does not claim to measure those capabilities.

## Canonical records and commands

`SetWorkspaceTrust` changes the workspace authority epoch. `SetPolicy` updates
a revisioned policy and authority epoch, including first configuration so old
context seals invalidate. `SetGrant` creates/updates a user grant with compare-and-set
semantics; stale actor/host/binding/policy/expiry inputs reject. User commands
cannot impersonate host rule origins. Typed `AuthorityDocument` records occupy
the existing `access` collection. Task/session references and approval-derived
grant provenance are validated by both canonical backends.

`Ask` commits the pending question and waiting task together. A paused task stays
paused. The headless JSONL handler returns a durable receipt without waiting for
terminal input; no highlighted option counts as consent. Questions bind actor,
operation/effect revision, steering, policy, expiry, controller/owner epoch and
workspace authority/binding.

`Decide` uses compare-and-set. An identical repeated answer returns the recorded
result without creating another decision or grant. A conflicting, stale, foreign
or expired pending answer grants nothing. An allowed answer creates one exact
grant in the same transaction as the decision, with a protected reference to its
approval. Answering never resumes a waiting/paused task. The canonical evaluator
preserves a prior denial for that exact scope/operation/policy/steering revision;
a broad grant cannot erase it.

Optional approval provenance fields preserve the serialized bytes of older
version-1 records when absent. Legacy and old-owner pending questions remain
inspectable but need a fresh question before new authority can be granted.
Existing workspace records and applied migrations are unchanged; the access
collection accepts a new typed document within its existing versioned envelope.
The explicit `document_type: vcp_authority_v1` discriminator separates typed
authority from legacy access documents, including those with a `data` field.
Rebinding invalidates host-specific grants rather than transferring authority.

The retained host follows its own acknowledged authority epoch, reads current
tool-policy revisions for questions/context seals, and keeps budget admission
separate. Native tool execution integration remains an explicit subsequent gate.

## Qualification

```powershell
pwsh -NoProfile -File scripts/test-policy.ps1
pwsh -NoProfile -File scripts/test-integration.ps1
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The policy runner enables actual store/accounting/history process-crash tests
and records 47 contracts, native versions and source hashes. It uses synthetic
identities and no provider credentials. See [the qualification report](../evaluations/p2-policy-increment.md)
for executed results, native regression scope and remaining acceptance.
