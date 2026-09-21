# P6-03 / M2 — Canonical local shadow execution

The canonical owner can explicitly fit and install a frozen task-local model,
then compare later admitted escalation evidence without a provider request or
helper reservation. `fit_local_shadow` is an offline operation;
`install_local_shadow` verifies the model against retained training sources.
`select_local_shadow` restores an explicit artifact pin after reopen, with the
same verification. Inference never updates model parameters.

The CLI decision profile accepts an optional `local_fit` artifact/digest pin in
shadow mode. Reconfiguration clears the old selection before applying this pin.
Remote qualification and credentials remain independent. Without an explicitly
selected local fit, existing behavior is unchanged.

For an admitted escalation, the owner prepares a bounded snapshot, computes on
a blocking worker outside its command loop, and revalidates before publication.
The two-second deadline, current task/input/steering, source context, authority,
retention, policy/catalog, selection and trigger identity must still match.
Pause and steering remain responsive during computation.

A deterministic result artifact identity permits one retained local result per
main admission, independently of remote comparison. The artifact contains exact
statistics and input/fit provenance; historical inspection reads those retained
bytes. Reopening does not replay triggers or select a model automatically.
Discarded late results may retain bounded historical skip metadata without stale
statistics; revoked access or pruned training sources deny capture.

Fit and result artifacts reference their source verifications. Results also
reference the fit, main attempt, task, admitted context and escalation evidence,
so source retention includes derived copies. Existing artifact purge/redaction
semantics apply without a new projection schema.

When both producers are enabled, results remain separately identified shadow
evidence. Their statistics are not averaged and neither changes routing, limits,
required review/checks or the admitted handoff. The local purpose remains only
repeated-strategy suspicion; serving qualification is false.
The combined response explicitly abstains from an agreement classification:
local suspicion and remote advice have no shared calibrated comparison.

M3 read-only forecasts follow this runtime increment. M4 must evaluate local and
remote purposes separately before any behavioral influence is considered.

Verification: all 19 shadow integration tests passed, including four new local
cases across Files and SQLite, pause during computation and actual source purge.
The CLI profile-validation test passed. Lifecycle compilation, formatting and
diff checks passed. The fast suite passed all nine cases
(`853410d3-e2e6-476b-bf02-25f65934d139`). No live provider qualification was run.
