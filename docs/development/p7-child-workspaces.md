# P7-04 child graph and workspace ownership

Status: implementation and connected native qualification in progress. P7-05
integration and P7-06 user controls/recovery retain their separate acceptance
gates. No optional M8 forecast is required for baseline child admission; M7 remains
deferred.

The controller's `CreateChild` command records a pending canonical task, graph
assignment and root-ledger allocation in one transaction. Graph revisions protect
dependency updates. Admission checks ancestry, acyclicity, acceptance, bounded
depth/concurrency, current parent state/steering, authority, policy, grant revisions
and allocation. Allocations subdivide the existing root cap; they are not charges.
Cancellation preserves all allocations, settled charges and unknown liabilities.

Native workspace inputs are captured as immutable, scoped artifacts before graph
creation. A host-only registration binds the child ID, unique root, disposable
absolute path, snapshot fingerprint and any explicitly registered Git metadata
owner. Materialization and observed native-directory identity form a separate
durable readiness receipt. A pending child cannot start before this receipt.
Pending-to-running admission precedes retained-thread startup; a consumed launch
ticket cannot silently create another child. Admission rechecks the complete
snapshot contents before launch, and pause/steering invalidates outstanding
preparation and launch tickets. Cancelled materialization retains its registered
partial directory for reconciliation rather than reporting it ready.

Snapshots preserve index bytes/modes separately from working bytes and scoped
untracked inputs. Capture excludes sensitive paths and ignored inputs, uses a
bounded before/after comparison, and holds the engine's resource claim against
its own writes. Native checks still detect human changes outside the engine.
Materialization never stashes, commits or resets the user's work. Git worktrees
use no checkout filters or hooks. Nested worktrees require their original metadata
owner registration; no arbitrary `.git` pointer supplies authority. Non-Git copies
include only explicitly selected inputs and use the same ownership and no-follow
checks. Unexpected files invalidate a replay's initial-content verification.

The broker resolves assigned tasks to their registered child roots, checks actual
paths/effects against the assignment, and projects current parent-root denials and
configured grant ceilings onto the isolated root. Exact-operation grants are not
broadened. Nested configured grants require an unbroken, current inheritance
chain. Read-only helpers cannot patch or execute. Child processes require qualified
filesystem enforcement; changing their working directory or using a reduced
isolation process profile does not establish an isolated write boundary.
The registered ownership marker is not an editable task file. Context assembly,
source validation and verification use the same assigned root.

The model assignment is an exact qualified model ID. A different configured
model is rejected before reservation or wire dispatch. Child setup inherits the
parent's provider request ceiling and deadline, while the graph's child deadline
and allocation remain additional limits. It does not import parent conversation
history into the child's context.

Unsupported Git index modes, unresolved conflicts, unavailable original objects,
unstable inputs and missing/replaced child directories remain explicit failures.
Failures leave their registered disposable path for reconciliation. These APIs do
not recursively delete worktrees, accept self-reported child verification as parent
success, or automatically restart a lost retained child. Read-only recovery resolves
registered native identities independently of expired execution grants/deadlines.

P7-05 preparation compares the actual child state with its captured base and the
current parent. It preserves findings separately from rejected edits, reports
three-way conflicts, and prepares version-bound edits through the existing file
broker. Each edit retains the observed parent index under a native read handle;
integration never writes that index. The parent inventory and versions are
revalidated before application; current parent verification is still required
after application. Submitted packet/plan/effect references remain in the graph
when the child is cancelled or its proposed edits are rejected.

Review packets accept historical string findings and version-1 structured
findings. Structured findings carry examined base/current fingerprints and paths,
a bounded line location, trigger, consequence, evidence references, uncertainty,
and a defect-versus-suggestion classification. Change causality defaults to
unknown; claiming introduction or a pre-existing defect requires indexed
comparison/reproduction evidence, not merely a source location on a changed line.
These remain untrusted claims. Preparation checks their shape, read scope and
revision attribution; it does not certify their truth. Read-only results undergo
the same native observation before their findings are retained without an edit.

The owner API can explicitly attach typed findings to a fresh observation. Plain
helper output remains unstamped legacy evidence with its canonical transcript
artifact ID and digest; it is never automatically converted into demonstrated
defects or invented examined revisions. Owner inspection reads up to eight
transcripts in canonical record order, with 4 KiB previews and explicit truncation;
full artifacts retain the evidence. Historical findings are labeled as not
re-examined against current files, even when the diff text is unchanged.

## Owner controls

`/agents` pages eight children at a time; `/agents 8` selects the next page.
`/agents focus <task>` inspects one child and `/agents follow <task>` keeps its
canonical status visible. `/agents pause|cancel|resume <task>` controls an
individual child. `/pause` fences the whole tree; `/resume` does not silently
resume independently held children. Views show node-local known, reserved and
uncertain cost, exact assigned model, workspace registration, active effects,
last durable activity and scheduling constraints. Viewing does not start work.
The same projection is available through
`vcp --format jsonl tasks agents <root-task> --offset 0`; existing
`tasks pause|cancel <child-task>` use the owning controller's stop path.

`/agents delegate <spec.json>` reads an explicit owner-authored JSON file, limited
to 64 KiB. The specification supplies no credentials or grants. Example:

```json
{
  "version": 1,
  "git": "C:/Program Files/Git/cmd/git.exe",
  "disposable_parent": "C:/vcp-child-workspaces",
  "objective": "Review the parser for malformed input handling",
  "acceptance": ["Report concrete findings with file references"],
  "mode": "read_only",
  "write_paths": [],
  "untracked_inputs": [],
  "allocation_usd": "0.10",
  "seconds": 120,
  "required_checks": []
}
```

The disposable parent must already exist. Allocation must fit the root budget;
the inherited root deadline remains an additional execution ceiling. For
`isolated_write`, supply exact permitted
write paths and applicable check IDs. Process-based child checks remain
unsupported without qualified filesystem enforcement; this limitation cannot be
converted into a successful child verification claim.

Read-only helper defaults can be selected without authoring a specification:

```text
/agents explore . 0.10 120 "C:/Program Files/Git/cmd/git.exe" "C:/vcp-child-workspaces" Locate parser entry points and explain the error flow
/agents review . 0.10 120 "C:/Program Files/Git/cmd/git.exe" "C:/vcp-child-workspaces" Review the current parser changes for supported defects
```

The arguments are read scope, USD allocation, seconds, native Git executable,
existing disposable parent, and objective. Both templates inherit the current
qualified model and permit reads only. The canonical initial objective retains
the template name, revision and exact guidance. A stale template revision fails
before graph creation. Explicit JSON specifications remain supported, including
optional free-form `role`, `read_paths` and versioned `helper` fields.

Currently `.` (the captured snapshot) is the supported helper read scope. Narrower
scopes fail explicitly because context assembly requires whole-snapshot read
authority; a requested narrow scope is never silently broadened. Untracked inputs
still require the explicit specification path. No setup recipe, process, dependency
installation or model switch is implied by selecting a helper.

`/agents` reports recorded materialization readiness separately from unavailable
child process verification, lists declared checks not run and retains the setup
reason. A recorded readiness receipt still requires native identity revalidation
before dispatch. Helper comparison and live usefulness qualification remain open.

Deliberate cleanup uses `/agents cleanup preview <task> <absolute-git.exe>`,
then `/agents cleanup apply <task>`. The child must be terminal and have no
retained runtime owner, dependent workspaces, unsettled root costs or active/
unknown effects. Cancelled children attached to the current owner require
close/reopen before cleanup. Root write authority and a running, current owner
are required; pause and owner loss fence each native removal.

Changed child content requires explicit `--reject-edits` on the preview. The
observed result bytes are retained before deletion, along with an immutable
cleanup intent and graph lease. Even integrated content is treated conservatively
as changed until explicitly rejected for disposable storage removal. The graph
keeps history/cost and references the retained result, intent and receipt; these
references protect their artifact retention. Cleanup does not prune Git metadata.

If removal is interrupted, locked or observes new edits, use
`/agents cleanup reconcile <task>` after resolving the diagnostic. Reconciliation
uses the original inventory and identities, never silently prepares a different
root. A completed receipt never deletes a new directory at the old pathname.
Bounded exact failure reasons are retained as append-only graph-linked evidence;
`/agents` shows the latest diagnostic after reopening without starting deletion.
Inspection, startup and reopening cannot initiate cleanup. Missing native paths
are reconciled only against a previously published cleanup intent.

After explicit root pause, `/agents recover <task> <absolute-git.exe>` observes
the registered native workspace and attaches a held child. It preserves edits
and does not restore baseline files over them. Resume the parent, then explicitly
resume the child. Missing or moved workspaces retain diagnostic artifacts. Failed
startup tickets pause their unbound child; failed setup after attachment requires
close/reopen recovery of the retained registration.

`/agents integrate <task>` observes actual child changes and retains a
conflict-aware packet and plan. Inspect their artifacts before
`/agents apply <task>`, which uses ordinary parent policy, approval and file
receipts. Rejected edits and useful findings remain inspectable. This command
does not certify parent completion. When children finish after a parent turn
ended, the parent pauses for explicit review and continuation.

Child public messages are captured as full `ChildTranscript` artifacts before
bounded UI notices. Private reasoning is not used for progress. Aborted or failed
turns pause the child instead of completing it. On terminal exit the owner fences
dispatch before dropping integration and child observers.

## Qualification

Relevant native tests are `vcp-repository/tests/child_workspaces.rs`,
`vcp-engine/tests/child_graph.rs`, and the lifecycle `support/child_agents.rs`
matrix, `support/child_recovery.rs`, and native CLI executable fixtures.
Local native evidence includes 37 repository/tool tests, eight engine graph
tests, connected child and integration matrices on both stores, exact-model
rejection before dispatch, fresh-owner recovery, and real PTY delegation with a
mock provider. Repository delivery checks passed at
`artifacts/p7-delegation-fast/5d466093-e771-42bb-9e9e-4265a372dcbd/manifest.json`.
These are component and connected-fixture results, not live U02/U03/U06 or P8
release qualification. Final regression checks, integration interruption cases,
multi-child terminal/recovery coverage and live usefulness remain separate gates.
