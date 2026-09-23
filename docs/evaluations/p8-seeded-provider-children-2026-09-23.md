# P8/M9 seeded provider and child boundaries — 2026-09-23

Status: all three formal selected rows passed; M9 and P8 acceptance remain partial.
This follows the [seeded boundary campaign](p8-seeded-boundaries-2026-09-23.md).
It uses synthetic loopback providers and disposable roots, not paid endpoints or
the production package. P8-05 and its downstream P9 prerequisite remain open.

## Provider scope

Four fixed seeds (`1`, `0x5eed`, `0x8a01`, `0xc0ffee`) on Files and SQLite drive
16 sequential model turns per trace. A required prefix plus generated ordering
exercises priced completion, completion without final usage, terminal HTTP error,
one/two retryable errors followed by completion, and retry exhaustion. The host
permits at most two retries; the retained adapter's own request and stream retries
are disabled. At most 384 HTTP requests can occur in this suite.

The literal fixture price is 100 micro-USD per request, with zero token rates and
a 10,000-micro root cap. The wire observer records bytes and ordinal before each
response and checks one unique submitted attempt/reservation with a durable send
intent. Independent expected counts, fixed-fee arithmetic and predecessor links
must match retained state. Callback errors are retained for assertion on the test
thread, not hidden inside the server task. Each new logical turn refreshes its
sealed context. These are sequential turns, not autonomous tool-loop or stall
detection evidence.
Missing usage, terminal HTTP errors and retry exhaustion deliberately pause the
root. An explicit owner resume precedes the next logical turn; an independent
terminal-state table and resume-count oracle check that continuation, while all
previous unknown charges remain in the accounting oracle.

A separate bounded test pauses during a predecessor-specific retry wait, closes
the owner and reopens. It must retain unknown liabilities, observe no additional
wire sends and refuse startup while paused. Cooperative close/reopen is not a
process-kill observation. It uses the same four seeds on both stores. Each trace
has four completed prefix turns (priced success, missing final usage and two
generated schedules), then one HTTP 429 with a five-second Retry-After wait
(the qualified delay ceiling). After acknowledged pause, observe for 5.1 seconds
and require no new wire send. An extra send before acknowledgement invalidates
the intended harness schedule instead of being misreported as a post-pause bug.
The maximum is nine physical sends per trace, 72 across the suite. This tests
unknown-liability preservation; late-usage reconciliation has its separate
accounting campaign.
Interrupted HTTP-error response captures also retain the unfinished-capture
startup fence. The reopened test must check that explicit admission refusal,
unchanged canonical state and unchanged wire count; it must not bypass the fence
to attach a replacement retained thread.

## Child scope

The same four seeds on both stores drive 20 operations per trace, 160 total.
Each trace admits two children with allocations of 400 under a root cap of 1,000;
an additional 400 allocation must be refused. No more than two loopback requests
per trace, 16 total, are permitted. Synthetic known request cost is 100 each.

Four parameterized seeded scenarios vary two sibling-order permutations,
read-only or isolated-write sibling pairs, selected child/root stops,
pause/cancel and stale/scope/dispatch probes. An independent model tracks child
IDs and graph edges, stop disposition, request outcomes and the shared root
liability. Mandatory operations cover allocation refusal, actual sends, stops,
root hold, cooperative reopen and attempted dispatch from a held recovered child.
No read, restore or attachment may silently resume it.
The root-stop scenario interrupts in-flight responses: retained unfinished
provider captures must fence new capture/admission even after reopen. The other
three scenarios drain responses before individually pausing or cancelling a
child, allowing actual recovered attachment while still refusing held dispatch.
Both branches preserve their independently observed wire and liability evidence.
The complete seed set must assert coverage of child pause, child cancel, root
pause, both selected child slots and both read/write scope modes; distinct seed
labels alone do not establish those branches.

Child delegation exercises a parent/child graph; it is not evidence for an
explicit conversation-fork command. Existing native unknown-effect/process-kill
fixtures retain their separate scope and are not relabeled as these seeded
cooperative traces.

## Required evidence

Each default test must print seed, backend and bounded attempted prefix, assert
that its declared operation classes occurred, and preserve independently observed
effects even on failure. Failed preparations or oracle assumptions remain in the
record. Formal execution uses the versioned P8 manifest, an unchanged source
snapshot, exact named-test discovery and no seed/prefix overrides. Unselected
matrix rows remain not-run.

Exploratory execution caught fixture assumptions before qualification. A child
request marker initially existed only in incoming turn text; the canonical sealed
context replaced that text, so the wire oracle correctly refused it. The fixture
now captures the marker in the sealed objective. Another assertion incorrectly
equated an inherited root runtime hold with an immediate durable child-state
transition. The control worker transitions the addressed root and holds its
subtree; owner recovery subsequently pauses nonterminal tasks. The oracle checks
those distinct states and still requires held dispatch to be refused. The failed
logs remain in `artifacts/p8-seeded-child-exploratory.log` and
`artifacts/p8-seeded-child-exploratory-2.log`; neither is passing evidence.
The third run reached reopen and correctly encountered the worker's unfinished
provider-capture fence. Before the next run, the schedule was revised to preserve
that stricter refusal for the in-flight root-stop branch and drain before
individual child stops in the other branches. This keeps the declared operation
and wire bounds while separating fenced recovery from attachable held recovery.
The third failed log is `artifacts/p8-seeded-child-exploratory-3.log`.
Provider exploration likewise corrected the fixture's assumption that uncertain
turns could continue without an explicit resume. A subsequent test-only type
conversion error was fixed before runtime; neither failed attempt qualifies a
provider row.
Resume exploration then required explicit canonical workspace trust and policy
records, which the simpler single-turn fixtures had not needed. Those records
are established before attaching the retained owner; resume checks remain intact.
The fourth child run passed its first complete scenario, then correctly refused
overlapping mixed read/write sibling scopes. The fixture now assigns disjoint
`left.txt` and `right.txt` scopes through the real delegation API; the conflict
guard remains unchanged. Its failed log is
`artifacts/p8-seeded-child-exploratory-4.log`.
The next attempted narrow read scope was also correctly refused: qualified child
context assembly requires whole-snapshot read scope. The supported scenarios
therefore use homogeneous sibling mode pairs, both read-only or both isolated
write with distinct disposable roots. Mixed sibling modes are not claimed.
That failed preparation is retained in
`artifacts/p8-seeded-child-exploratory-5.log`.
The sixth child attempt failed at link time because the shared Windows test
executable was still running the provider campaign. No child scenario executed;
subsequent native runs are serialized after executable release.
The seventh child run passed the three settled Files scenarios, then showed that
the interrupted-capture branch refuses retained session startup itself, before
binding. Its recovery probe must therefore observe that exact startup denial
without constructing a replacement retained session. The failed run remains in
`artifacts/p8-seeded-child-exploratory-7.log`.

## Targeted execution

The provider burst test passed all eight traces, 128 logical turns and 252
physical sends. Its independent disposition oracle also checks 60 explicit
resumes. Log: `artifacts/seeded-provider-exploratory-5.log`; the combined run still
contains the earlier stop-test failure and is not an overall passing campaign.
The corrected exact stop test passed all eight traces with 58 physical sends in
79.49 seconds: `artifacts/seeded-provider-stop-exploratory-6.log`.
The child test passed all eight scenarios in 48.87 seconds:
`artifacts/p8-seeded-child-exploratory-8.log`. Its 160 operations include 16
assignments and observed requests, eight over-allocation refusals, eight stale
stop refusals and eight reopens. Six owners reopen with held recovery; two remain
capture-fenced. There are ten actual held child attachments, four capture-fenced
child binding refusals and two cancelled-child recovery refusals.
Formal source-bound qualification passed all three selected rows:
`artifacts/p8-seeded-provider-child-campaign/1c909e88-fd7d-480c-99ad-bc9e6490d728/manifest.json`,
SHA-256 `f89b8bc7e10638d5d35d50b248f9420d914f2bca96199496f784b144bf47d0c9`.
Before/after source digest matched
`bec4f12eb30d1ac6908f6c1b971f60be636c72846d79765b81db673d4e3ce044`.
The full manifest correctly remains **incomplete**: three passed, zero failed,
50 unselected rows not-run. It does not supersede their existing evidence.
All 17 fast delivery groups passed:
`artifacts/p8-provider-child-fast/8f9636c2-084d-46f2-b46b-fd94746ff196/manifest.json`.
Formatting, diff checks and documentation validation also passed. Independent
source review found no remaining blocking issues; no product code changed.

## Remaining acceptance

This campaign cannot establish live endpoint quality, actual packaged delegation,
minimum hardware, clean Windows, physical-volume faults, full production offline
operation or owner usefulness/architecture-fit judgments. Previously prepared
paid plans still require their separate explicit execution authority.
