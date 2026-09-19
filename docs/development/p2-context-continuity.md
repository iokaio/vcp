# Context continuity and deterministic compaction

P2-08 is in progress. The current pure compaction module creates bounded
historical previews of completed tool pairs. Retained-host integration,
current-state pinning and complete continuity acceptance remain pending.

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
