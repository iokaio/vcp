# CS-3 implementation phases

Status: in progress; synthetic browser diagnostics only, no browser qualification
or six-skill acceptance claim.
Contract: [plan 24, CS-3](../plan/24-skills-follow-on.md#cs-3--browser-execution-and-six-skill-acceptance).

On September 26, 2026 the owner authorized implementation of the full CS-3
plan, including careful investigation and provisioning of the proposed restricted
Windows account/firewall boundary. Delivery stops at a committed review PR;
merge is not requested. That approval does not make the proposed boundary
qualified, establish a dollar/call ceiling, or invalidate earlier failed evidence.

Current evidence is tracked in the [native checkpoint](cs3-native-dom-checkpoint.md)
and [six-skill readiness ledger](cs3-six-skill-readiness.md). The bounded HTTP
server, non-default candidate and separate six-case WEB preparation inventory
have offline tests. These are prerequisites; none closes a phase's full exit gate.

## Phase 1 — Prove the proposed Windows boundary

The [boundary review](cs3-account-boundary-review.md) found that the proposed
account/firewall mechanism does not establish the promised access restrictions.
No account, firewall or existing filesystem permission was changed. Independently
checked local WebView2 startup evidence supports continuing inside the original
zero-capability AppContainer. The fresh diagnostic source under
`src/tests/support/windows/webapp/` uses that route. The account provisioning
conditions below remain conditional requirements, not a selected implementation.

Review the restricted-account, filesystem and per-user firewall assumptions
against the actual APIs before changing this host. In particular, an ordinary
account can inherit access through Users/Everyone, and a firewall rule being
installed does not establish that it filters loopback or every child token.
Chromium's internal sandbox must remain enabled.

Before provisioning, bind the exact new account/rule names, SID ownership,
private directory targets, browser inventory and expected host state to a
reviewable proposal. Check administrator availability and effective firewall
policy. Record existing state without recording credentials. Refuse collisions,
unexpected policy, reparse paths, or changed identities. No existing user account,
browser profile, firewall default, global ACL or existing sandbox is a cleanup
target. Any account credential stays outside Git and diagnostic output.

Use a bounded native probe with synthetic files and listeners. Prove allowed
runtime/profile access and the single owned endpoint, plus denied host reads,
writes, IPv4/IPv6, other loopback services, external destinations, and child
escape attempts. Prove startup owner-loss cleanup. Keep rules effective until
all owned descendants are independently drained; never remove protection while
the browser could still be alive. Recovery may remove only exact newly created
objects after matching their saved identity/state, and must report unresolved
objects rather than claiming cleanup. Preserve the two historical unresolved
profiles described in the [feasibility record](cs3-browser-feasibility.md).

Exit: a viable boundary with actual Windows evidence, or a precise failed probe
and a concrete alternative requiring new provisioning. No browser launch against
untrusted project content until this phase passes.

## Phase 2 — Owned server and browser execution

Implement a bounded server over an explicitly hashed, immutable synthetic file
inventory. Bind an owned endpoint exclusively; reject unexpected origins,
methods, request paths and oversized input. Never execute project scripts or
forward a request to another service. Test changed files, links, occupied ports,
pending requests, deadlines and preservation of unrelated listeners.

Build the browser controller only against the qualified phase-1 boundary. Bind
runtime files, private profile and process identities; constrain transport,
output, requests and time. Test real startup, failure, cancellation, pause,
owner loss and independent descendant drainage. Receipt validation must keep
controller faults distinct from candidate failures. Missing or changed browser
prerequisites fail explicitly.

Exit: native Windows server/browser lifecycle and enforcement tests pass. Server
unit tests alone do not establish browser isolation.

## Phase 3 — WEB fixtures, browser oracles and original skill

Add a separate WEB cohort without editing the frozen CS-2 developer fixtures or
campaign. Use two normal tasks, boundary, hostile, missing-browser and near-miss
cases, compared with no skill and the existing testing skill. Keep independent
oracles outside candidate workspaces. Freeze hashes and assignments before live
evaluation, after the executable interface is established.

Author the non-default `webapp-testing` candidate and test descriptor integrity
and explicit selection. Deterministic browser assertions cover form submission,
keyboard/focus, validation, filters, loading/error/retry, viewport overflow and
reduced motion. Regrade retained CS-2 UI workspaces using a separately bound,
read-only import; preserve all historical outcomes. Document that DOM assertions
and screenshots cannot establish model pixel inspection or human visual review.

Exit: complete WEB fixtures and real-browser oracle controls; candidate ready for
prospective comparison. A prepared descriptor is not a qualified default skill.

## Phase 4 — Six-skill comparison acceptance

Correct the recorded CS-1 and CS-2 candidate defects prospectively and preserve
their earlier versions, failed comparisons and halted campaign claims. All five
existing candidates remain unqualified; the sixth requires its first comparison.
Independent review must check correctness, preservation, authority, secrets and
evidence honesty as well as benefit over both baselines on a normal task.

Prepare a concrete source/fixture/model/provider/toolchain proposal with dollar
and call ceilings before paid dispatch. Prior allocations cannot be replayed,
reassigned or transferred. The owner's authorization to implement is not an
unbounded paid evaluation allowance. A failed or tied comparison stays failed
or unqualified; do not promise that implementation guarantees a positive result.

Exit: all six required comparisons qualify, or retain explicit unqualified
results and report that the current CS-3 exit condition is unsatisfied.

## Phase 5 — Distribution, review and PR

Only qualified bytes enter the builtin catalog. Verify the exact native package,
install/upgrade/rollback, offline discovery, explicit activation and revocation,
source precedence and integrity for all six additions. Run relevant repository
gates, review the complete diff and ensure host recovery evidence is retained.

Commit attributable increments and create the requested review PR. Its title,
description and test record must distinguish completed phases from unavailable
or failed acceptance gates. Keep CS-3 open if any required gate is unresolved;
a partial PR must not claim completion. Stop before merge.
