# P2 completed-response tool boundary

P2-05 is in progress. The retained Codex loop remains the scheduler. Its host
admission trait now offers `requires_completed_response()`, defaulting to false
for existing callers. `CanonicalHost` enables it. Registered prepared tool
wrappers and automatic context assembly are subsequent work; the production
canonical host still denies model-requested tools.

When this requirement is enabled, receiving a completed tool item records the
observation but defers construction of its execution future. This distinction
matters because constructing the retained future starts a Tokio task. Only
draining an accepted response can construct those tasks. A failed stream or a
cancelled turn drops deferred calls before the drain. The existing response
permit must accept completion before the loop receives a successful terminal
event, so failed capture/validation/accounting cannot admit deferred effects.

This boundary does not replace prepared-operation policy, source checks, owned
dispatch or effect receipts. It controls initial execution eligibility. Observed
partial calls remain history; they never become replayable execution authority.
The default retained behavior is unchanged without the explicit host requirement.

The implementation modifies three already selected Apache-2.0 Codex files:
`ext/extension-api/src/work_admission.rs`, `core/src/stream_events_utils.rs` and
`core/src/session/turn.rs`, under the committed `codex-rs/` tree. Original VCP
host configuration and synthetic tests live in `vcp-lifecycle`. Patch 0020,
selection/result hashes and notices record the maintenance change. No additional
upstream source or external dependency is imported.

The native test consumes a tool item followed by an observable text event while
the terminal response is withheld. It checks both the dispatch gate and an
independent filesystem marker. Completed, truncated, rejected and interrupted
responses exercise the strict host path on both stores. A separate default-mode
control demonstrates that existing retained callers still start tools while a
response is streaming. These use a test-only marker tool and private loopback
provider; they are not production tool-wrapper or paid-provider acceptance.

See the [qualification report](../evaluations/p2-response-tool-boundary.md),
[authority guide](p2-policy.md) and [P2 plan](../plan/05-openrouter-and-session-loop.md).
