# P6-03 owner escalation declarations

Owner declarations now feed the existing bounded escalation selector and handoff.
They express declared complexity or an exact unsupported capability; model output
does not create these declarations. The current local owner invokes the existing
`routing_control` boundary with a typed request:

```json
{
  "operation": "declare_escalation",
  "declaration": {
    "command": "owner-command-id",
    "task": "task-id",
    "expected_revision": 0,
    "steering": 0,
    "declaration": { "kind": "declared_complexity" },
    "evidence": ["artifact-id"]
  }
}
```

Capability declarations instead use
`{"kind":"unsupported_capability","capability":"exact_key"}`. Capability
identifiers contain 1–128 ASCII letters, digits, underscores or hyphens. At least
one current catalog candidate must explicitly support the named capability;
ordinary model, provider, quality, privacy and budget restrictions still apply.
Each declaration references 1–63 distinct, complete, retained artifacts from the
same task. The host captures the typed input as a separate evidence artifact and
binds the declaration to its digest, task revision, steering, authority, deletion
epoch, workspace binding and latest canonical main/child attempt.

Declarations require running tasks and ancestors. They neither resume paused
work nor change authority. New steering, source changes, a new predecessor or
pause/resume revisions make an old pending declaration stale. A fresh explicit
declaration can supersede stale pending records. Reusing a command with identical
input returns the retained record; reusing it with different input fails.

The next routing boundary durably consumes the declaration before selection.
Consumption is at most once: a crash, strict-pin rejection, insufficient budget or
other failed admission cannot replay it. The owner can inspect the result with:

```json
{"operation":"escalation_declarations","task":"task-id"}
```

Inspection uses the same current-source checks as consumption and joins actual
escalation admissions. `consumed_without_admission` is distinct from `admitted`;
neither claims task completion. Failed scheduling requires a fresh declaration.
An admitted capability remains a required selector capability for subsequent
requests in the same task and steering revision, derived from the existing
declaration/admission records. It cannot silently disappear after the first
handoff or when escalation configuration is disabled.

Records reference their task, predecessor and evidence in the canonical store.
Purged declaration projections and incomplete or purged source artifacts cannot
be consumed. Inspection does not reconstruct deleted evidence. Existing root
counters, strict pins, pause barriers, handoff assembly, request reservations and
late usage accounting remain authoritative.

## Verification

The focused Files/SQLite declaration test passed, including idempotent creation,
durable consumption across reopen, no implied admission, stale predecessor,
read-only inspection, paused-task rejection, explicit-resume behavior, replacement
of stale declarations, and inspection/claim agreement after authority, binding and
deletion changes (`artifacts/p6-owner-declaration-tests.log`).

The native retained-host routing matrix passed with owner complexity, capability
and strict-pin cases on both stores (`artifacts/p6-owner-declaration-host.log`,
35.69 seconds). Its capability case has three candidates and three actual
requests: after the first handoff, the cheaper unsupported alternative remains
ineligible and the following request retains the required capability.
Its synthetic transport and compatibility labels exercise admission and handoff;
they do not qualify actual model capability or live evaluator usefulness.
