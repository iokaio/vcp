# Context continuity and deterministic compaction

P2-08 is in progress. The pure compaction module creates bounded historical
previews of completed tool pairs. Its retained-host integration passed native
integration contracts; complete continuity acceptance remains pending.

`vcp_context::compaction::compact` accepts only adjacent, complete, unique
tool-call/result pairs from one canonical scope. It rejects current objectives,
constraints, instructions and task-state parts. Those fields must remain outside
any summary. It preserves a configured number of recent pairs in full and creates
UTF-8-safe previews of older arguments/results with explicit omitted byte counts
and original artifact references. The previews are untrusted historical data,
never proof that an operation succeeded or authority to execute another one.

Every input artifact remains a dependency, including sources omitted from the
preview. A saved projection records its algorithm/configuration, revisions,
original history digest, source hashes and content-byte gain. Revalidation
recomputes the projection and rereads its sources through an injected reader that
must enforce current canonical history access. Changed steering, scope, deletion
or authority, altered bytes, revoked reads and modified saved summaries reject
reuse. Only a complete capture of the exact preview can become a context part;
the constructor requires the process-local source-validation result and always
assigns untrusted history provenance. That result cannot be deserialized from a
saved summary.

The module does not mutate original history. Insufficient savings return no new
projection, allowing the owner to preserve its prior valid view and avoid
repeating no-gain work. Bounds are 512 input parts, 8 MiB of selected content and
64 MiB of referenced source bytes. Gain measures content bytes; the owning
adapter must separately recount the actual provider request and record its
qualified token estimate before admission. This module issues no model requests.

Four local contracts cover pair preservation, untrusted provenance, revision and
access changes, tampering, current-field/unfinished-pair rejection and no-gain
behavior. They and all 27 context contracts passed with native Rust 1.98.0; see
the [qualification report](../evaluations/p2-context-compaction.md).
Run `pwsh -NoProfile -File scripts/test-context.ps1` for the registered
context contracts. No new dependency, model asset or upstream import is used.

## Retained owner integration

After configuring coding and the owner's verification baseline,
`CanonicalHost::configure_continuity` enables deterministic compaction with
explicit bounds. Configuration is captured and cannot be replaced within the
same coding setup. It adds no model tool or remote helper request.

Each assembled request keeps current objective, task state and scoped instructions
outside summaries. A separate mandatory observed-state part contains the original
baseline and current disk-source manifests, concise effect records, full unresolved
or stopped effects, pending-charge details, quantitative ledger and recorded verification evidence.
Pending charges include request/reservation identities, quoted reserve, observed
charge, held liability and uncertainty, with hashes of complete canonical records.
History digests retain attribution to complete canonical accounting/effect records.
The snapshot precedes admission of the new request, which obtains its own ordinary
reservation. Historical check records still require current applicability checks
before completion.

The adapter validates original history access and source bytes, captures the
summary and its immutable projection, and compares both actual serialized provider
requests before selecting the compacted view. Gain uses the existing conservative
byte estimate and is explicitly marked estimated. Repeated no-gain inputs do not
re-run projection work. Original pairs and provider evidence remain unchanged.

Current source/access checks fence tool preparation, and source plus canonical
effect/accounting checks fence request admission. Reopening requires deliberate
resume and fresh owner configuration; it reconstructs original pairs and builds
a new projection under current revisions. A saved summary grants no access or
execution authority. Pending calls must be reconciled before assembly.

The native regression exercises a long read trace, a later user correction and
source edit, two reopens and an uncertain provider charge on both stores. A second
trace requires an oversized request to pause without another HTTP call or loss of
the existing liability. These four traces passed, as did the broader 44-contract
native integration suite. This increment represents disk changes through
source manifests; full current text diffs, Git/index and editor state, broader
decision/acceptance projections and incompatible-provider handoff still need
qualification. The installed CLI remains outside this internal host increment.
