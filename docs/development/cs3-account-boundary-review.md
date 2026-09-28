# CS-3 restricted-account boundary review

Read-only review, 2026-09-26. No account, group, firewall, WFP, ACL, registry,
browser, server or Windows-feature change was made. The current process is not
an administrator and has only ordinary low-privilege token rights. Windows
Sandbox and its command-line client are not installed in the observed host.
Those observations describe this host only and do not authorize provisioning.

## Disposition

Do **not** select the previously proposed dedicated-account plus Windows Firewall
boundary for CS-3. That recommendation was overstated in two material ways:

1. The vendored firewall code's `INetFwRule3.LocalUserAuthorizedList` is
   documented as an authorized-user list **for an AppContainer**, not as a
   general per-user selector for an ordinary desktop-account block rule. Merely
   setting and reading that property back does not prove that a desktop browser's
   traffic matches the rule, with or without IPsec.
2. A normal local account, even one with few privileges, ordinarily retains
   enabled `Everyone`, `Users` and `Authenticated Users` SIDs. It can therefore
   read files granted to any of those principals. The vendored restricted-token
   construction also includes `Everyone` among its restricting SIDs so that
   ordinary runtime dependencies remain usable. It does not create a deny-by-
   default filesystem namespace and cannot establish “no arbitrary host reads.”

Direct Windows Filtering Platform (WFP) filters can, in principle, match a user
SID at ALE layers without IPsec and can observe ordinary loopback traffic. That
is a narrower network mechanism worth one reversible synthetic probe, not a
complete browser boundary. It does not repair the filesystem problem.

The first next browser increment should instead remain inside the existing
zero-capability AppContainer and pursue the provisional WebView2 DOM/form/origin
probe. Unmerged local evidence under
`artifacts/cs3-webview2-disposition-worktree/` reports one GUI-host attempt that
reached readiness with seven of seven held process identities verified and then
independently drained. That evidence is provisional and does not establish DOM,
origin, owned-server or internal Chromium-sandbox acceptance, but it invalidates
the broader assumption that all installed Chromium-family routes fail the
existing outer boundary. No account or firewall mutation should precede this
same-boundary follow-up.

## Repository findings

### Process ownership is reusable but not access isolation

`src/crates/vcp-lifecycle/src/process.rs` supplies useful VCP-owned pieces:
suspended launch followed by job assignment before resume, process-count and
time/output limits, kill-on-drop behavior and a query-zero cleanup check. This
is not atomic assignment at process creation. The CS-3 diagnostic instead uses
`PROC_THREAD_ATTRIBUTE_JOB_LIST` and additionally retains completion-port
acknowledgement and held process identities. These mechanisms bound lifetime and resources; neither
a Job Object nor a PID census restricts filesystem or network authority.

Any future browser experiment must preserve the stronger fixture requirements:
held handle plus creation time, image path/hash, token, owned-job membership,
cumulative job accounting, independent `ACTIVE_PROCESS_ZERO`, and cleanup that
stays pending when any observation is missing. The two unresolved historical
profiles remain untouched.

### Vendored account/token code does not hide the host filesystem

The vendored provisioning code creates an ordinary `USER_PRIV_USER` account and
adds it to both the sandbox-specific group and built-in `Users`. Its restricted
token removes privileges and uses restricting SIDs, but the restriction list
contains capability SIDs plus the account SID, logon SID and `Everyone`.

Microsoft documents that a restricted token receives two access checks and both
must succeed. This is useful only when the restricting-SID check is itself
narrow. Including `Everyone` allows the restricted half of the check to succeed
for every object readable by `Everyone`; enabled ordinary groups can likewise
satisfy the normal half. Marking groups deny-only does not turn their allow ACEs
into denials. See [Restricted Tokens](https://learn.microsoft.com/en-us/windows/win32/secauthz/restricted-tokens)
and [SID attributes](https://learn.microsoft.com/en-us/windows/win32/secauthz/sid-attributes-in-an-access-token).

The repository's explicit deny-read ACL support protects enumerated paths. It
cannot safely enumerate every present and future host-readable path, inherited
ACE, hard link, reparse target, registry object, named object or device. Applying
denies broadly to host roots would also mutate user/system policy and is expressly
outside this work. A private profile and scratch directory constrain writes;
they do not revoke reads granted elsewhere.

A capability-only restricting SID would be closer to deny-by-default, but a
desktop Chromium/WebView runtime could not load ordinary Windows and runtime
dependencies unless those objects also admitted that SID. Broadly adding such
ACEs would recreate the unwanted host grant and materially modify system/runtime
ACLs. This is not a viable CS-3 increment.

### The vendored firewall rules are not evidence for desktop-user scoping

`src/third_party/codex/codex-rs/windows-sandbox-rs/src/setup_provisioning/firewall.rs`
creates Windows Firewall rules and assigns an SDDL value through
`SetLocalUserAuthorizedList`. It verifies that the property can be read back and
that local policy reports modifiable. Its tests validate COM acceptance and port-
range construction; they do not launch a process under the target account or
observe denied traffic.

Microsoft's [`INetFwRule3`](https://learn.microsoft.com/en-us/windows/win32/api/netfw/nn-netfw-inetfwrule3)
and [`put_LocalUserAuthorizedList`](https://learn.microsoft.com/en-us/windows/win32/api/netfw/nf-netfw-inetfwrule3-put_localuserauthorizedlist)
contracts describe this property as a list of authorized local users for an
AppContainer. The related Windows Firewall guidance uses user authorization with
authenticated/IPsec rules; it does not establish that the property scopes a
plain desktop-account block rule. Therefore property round-trip is insufficient
and the account-scoped effect must be treated as unproven.

The separate vendored `wfp.rs` does use `FWPM_CONDITION_ALE_USER_ID`, but only for
selected ICMP, DNS and SMB blocks. It does not implement deny-all external
traffic or exact-origin loopback policy. It cannot be reused as-is.

### Direct WFP is technically testable, but network-only

Microsoft documents ALE as the WFP location where filters can match application
and user identity. `FWPM_CONDITION_ALE_USER_ID`, remote address, remote port,
protocol and loopback flags are available at `ALE_AUTH_CONNECT`; ordinary and
AppContainer loopback are separately identifiable. See
[Application Layer Enforcement](https://learn.microsoft.com/en-us/windows/win32/fwp/application-layer-enforcement--ale-),
[available filtering conditions](https://learn.microsoft.com/en-us/windows/win32/fwp/filtering-conditions-available-at-each-filtering-layer),
and [filtering condition flags](https://learn.microsoft.com/en-us/windows/win32/fwp/filtering-condition-flags-).
These contracts do not require IPsec for a direct ALE user condition.

A correct exact-target policy cannot install “permit target, block everything”
as two ordinary filters and assume the permit wins. WFP's documented arbitration
makes an ordinary block a hard block. The block filters must instead describe
the complement so that the approved flow matches no VCP block:

- block all non-loopback IPv4 and IPv6 destinations for the synthetic SID;
- choose exactly `127.0.0.1` for the probe and block every other IPv4 loopback
  address plus all IPv6 loopback, including the allowed port;
- for `127.0.0.1`, block TCP remote ports other than the one pre-bound target;
- block all UDP, raw/ICMP and account-owned listen/bind operations not required
  by the probe;
- do not use a hostname, proxy, redirect, DNS lookup or inherited connection.

Filters must cover both address families and be installed before token launch.
Existing flows must not be reused. Results require independent socket canaries;
filter installation, enumeration and rule read-back are setup evidence only.
See [filter arbitration](https://learn.microsoft.com/en-us/windows/win32/fwp/filter-arbitration)
and [ALE stateful filtering](https://learn.microsoft.com/en-us/windows/win32/fwp/ale-stateful-filtering).

## Bounded network probe and rollback

This probe is optional evidence for a future native network primitive. It must
not delay or replace the same-boundary WebView2 work, and it cannot qualify the
account boundary by itself.

### Preparation

1. Build a source-reviewed synthetic client and two owner-side loopback servers.
   They exchange public non-secret nonces only. Freeze their source and binary
   hashes before elevation.
2. Prepare a unique run identifier and exact names for one temporary local user,
   one temporary group, one dynamic WFP provider/sublayer and one owned scratch
   directory. Reject collisions rather than adopt existing objects.
3. Record the pre-state by exact identity: absence of those account/group/WFP
   objects, server port ownership, current BFE state and scratch absence. Do not
   inventory unrelated accounts, paths or firewall policy in retained output.
4. Require an explicit elevated helper. The current process is non-admin and
   cannot perform this setup. The helper accepts only the frozen manifest and
   does not provide a general command channel.

### Execution

1. Create the temporary noninteractive user with a random in-memory credential,
   no administrative or remote-login membership, then log on and form the exact
   restricted primary token. Record only SID/token attributes, never the secret.
2. Open a **dynamic** WFP session and create run-unique nonpersistent provider,
   sublayer and complement block filters scoped by `ALE_USER_ID`. Microsoft
   documents that objects in a dynamic session are removed when the session
   closes or its process terminates: [WFP object management](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management).
3. Bind the approved server to `127.0.0.1` before launching the client, retain its
   process/socket identity and reject an occupied port. Bind the second server
   to a different loopback address or port as a denial canary.
4. Launch the synthetic client atomically in the owned no-breakaway job. Require
   success only to the exact approved address/port and denial for alternate
   loopback address, alternate port, IPv6 loopback, UDP and a controlled external
   address. Verify the user can neither listen nor connect through a proxy.
5. Independently inspect WFP net events where available, but use actual socket
   outcomes as the oracle. A timeout, policy conflict, unsupported condition,
   unexpected allow, missing event, stale flow or incomplete process drainage is
   failure/inconclusive, never a reduced-isolation pass.
6. Separately run read canaries against synthetic files whose DACLs grant
   `Everyone`, `Users`, `Authenticated Users`, only the controller, and only the
   run capability. The expected broad-group reads demonstrate why the account
   route is not a filesystem boundary; do not “fix” the test by changing host
   ACLs.

### Rollback and proof

Rollback order is fail-closed and idempotent:

1. terminate the owned job and independently establish zero descendants;
2. stop and drain both owned servers;
3. close the dynamic WFP engine handle, then independently enumerate by the
   run-specific keys and require that all dynamic filters/sublayer/provider are
   absent;
4. close logon/token handles, disable then delete only the exact temporary user,
   and remove only the exact temporary group if it is empty and run-owned;
5. remove the exact owned scratch directory after resolving it and proving it is
   inside the run root; retain it if ownership or drainage is uncertain;
6. compare the exact pre/post identities and emit a bounded receipt with no
   password, unrelated account list, machine-specific path inventory or host
   firewall dump.

If account deletion, dynamic-filter removal, server survival checks or process
drainage cannot be proven, stop with cleanup pending. Do not proceed to Chromium.

## Safer immediate route: same-boundary WebView2

The provisional WebView2 evidence uses the existing zero-capability AppContainer,
exact outer SID, private writable profile/scratch, atomic job assignment, held
process identities and independent drainage. The smallest follow-up should use
an in-memory fixed HTTPS origin through `WebResourceRequested`, a fixed DOM/form
oracle and host-side WebView2 CDP calls, with no listener, DNS, browser download,
debug port or new capability. Microsoft documents
[local-content interception](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/working-with-local-content),
[resource interception](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/webresourcerequested),
and [host CDP calls](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/chromium-devtools-protocol).

This increment must still fail closed on an unknown method/resource, bind every
event to the navigation identity, use protocol keyboard input rather than DOM
`.click()`/value assignment, inspect a bounded accessibility tree, and retain
the prior image/token/job/cleanup checks. It proves neither physical keyboard
input nor visual quality. The serviced Evergreen runtime's continuous identity,
renderer-internal restrictions and every-helper coverage remain explicit gaps.

An owned HTTP server is a later separate profile. Under zero network capability,
the browser must not receive loopback permission. A trusted, narrowly typed
owner-side bridge may translate fixed route IDs over the inherited private pipe
to one newly owned server, with no arbitrary URL, redirects, proxy, credentials
or adoption of an existing listener. Whether this mediated profile satisfies
CS-3's product intent must be recorded before it is claimed as browser/server
qualification.

## Exact unresolved conditions

CS-3 cannot accept an account, WebView2 or VM boundary until the applicable items
below have native evidence:

- provisional WebView2 source/receipts are integrated and independently audited;
- complete held-identity coverage includes short-lived helpers and renderer-time
  token, integrity and mitigation observations;
- the installed serviced runtime is bound against replacement/change without
  mutating its ACLs or overstating immutability;
- fixed DOM, real protocol input, accessibility and hostile-origin denial pass;
- owned-server startup/readiness, occupied-port preservation, long polling,
  timeout, cancellation and owner-loss cleanup pass under a selected transport;
- account-scoped direct WFP filters, if retained, pass exact IPv4/IPv6/loopback
  canaries and complete dynamic rollback;
- no native-account design is called a filesystem boundary unless it denies
  broad-group host reads without broad host ACL mutation;
- Windows Sandbox, if later selected, is separately provisioned and qualified
  with networking, clipboard, vGPU, audio/video/printer redirection disabled,
  only frozen read-only input mapping and one narrow output channel. Microsoft
  warns that mapped folders expose host data and writable mappings persist guest
  changes: [Windows Sandbox configuration](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file).

Until those conditions are met, retain explicit `not_run`/unqualified results.
No finding here authorizes a sandbox-disable flag, browser capability expansion,
signed-in profile, automatic download, persistent firewall exception or broad
host ACL rewrite.

## Pre-run review of the proposed WebView2 DOM integration

Follow-up read-only review of the source in
`artifacts/cs3-webview2-gui-draft-543d3d81b8174feb8230639609a1646c`.
The verified startup receipt and input manifest support using this source as the
baseline. They do not make every startup implementation choice suitable for a
larger DOM evidence stream. Do not run the new native probe until the must-fix
items below are implemented and covered by pure/offline tests.

### Must fix before the next native attempt

1. **Contain the worker against controller loss.** The PowerShell controller
   currently starts its worker with `Process.Start`; the worker is not atomically
   assigned to a controller-owned kill-on-close job. Polling the pinned controller
   handle in `observe` and `WaitForDrainAcknowledgement` is useful detection, but
   it cannot run while the worker is blocked in a synchronous pipe write, live
   tree traversal, WebView/native call or an unexpected wait. A controller crash
   can therefore leave the worker holding the sole browser-job handle.

   Create the worker suspended and atomically assign it to a separate outer
   guardian job owned only by the controller, with kill-on-close and no breakaway,
   before resume. The worker may then create its existing nested browser job.
   Record and independently verify both memberships. Controller loss closes the
   guardian job and kills the worker; worker death closes the sole inner-job
   handle and kills browser descendants. If nested-job behavior or atomic worker
   assignment is unavailable, the attempt is owner-loss-unqualified and must not
   run as acceptance evidence.

2. **Remove trusted recursive traversal of browser-writable trees.** `Scratch`
   calls `RegularTree` and then recursively enumerates the profile/temp by path.
   The check and traversal are separate operations. A browser-writable reparse
   point can appear between them, redirecting the trusted worker's traversal.
   File-count, depth and enumeration-time limits are also absent; a tree of many
   zero-length entries defeats the byte-only ceiling.

   Do not use `SearchOption.AllDirectories` on a live untrusted tree. If live
   accounting remains necessary, walk from held directory handles, open every
   child relative to the held parent with reparse following disabled, verify
   volume/file identity, and impose exact entry, depth, byte and elapsed-time
   ceilings. Treat any race, inaccessible entry, reparse point or identity change
   as failure. Prefer not traversing content at all during the DOM phase when
   WebView2 can supply the required evidence without it. The current 64-MiB scan
   is an observed threshold, not an enforced storage quota; record that limitation
   and do not claim hard scratch containment from polling.

3. **Define a new bounded evidence protocol rather than widening startup JSON.**
   Startup host records are limited to 2 KiB and a fixed ten-field schema. The
   proposed 16-KiB DOM and accessibility projections cannot be inserted into
   that record or accepted as an arbitrary new object. Define a versioned DOM
   protocol with exact allowed event types and fields. Large values use fixed
   chunks containing nonce, artifact kind, sequence, offset, total byte count,
   whole-value SHA-256 and bounded base64 payload. Reject gaps, overlaps,
   duplicates, inconsistent totals/hashes, data after completion and incomplete
   values. Reconstruct under the supervisor's independent cumulative caps before
   parsing with an exact depth/property/type schema.

   Count actual UTF-8 bytes, including framing/newlines. The host reader currently
   increments a character count, which is not a byte ceiling for Unicode DOM
   data. Keep the outer controller limit independent and at least as strict as
   the sum of declared per-artifact ceilings. A ceiling fault must latch failure,
   terminate the owned job and preserve only a bounded diagnostic prefix.

4. **Make the supervisor, not browser/host prose, decide acceptance.** The host
   and all DOM/CDP responses are untrusted evidence. Do not accept a host-provided
   `passed`, role label, count or summary. The supervisor must validate the exact
   navigation identity and source/response hashes, reconstruct DOM and AX data,
   and compare typed values to a frozen oracle. Implement an explicit phase
   machine with required order and cardinality: interception installed, intended
   navigation started/completed, DOM ready, initial oracle, each fixed input
   command and resulting oracle, expected hostile transition denied, approved
   document still current, then stop/close. Reject duplicate, missing, late or
   out-of-phase success events. Retain independent process identity/coverage and
   cleanup as separate mandatory gates.

5. **Fail closed on every intercepted request and asynchronous callback.** Install
   all-resource interception, navigation, popup, download, permission and external-
   scheme handlers before the intentional navigation. Canonicalize the URI as a
   URI, not a string prefix. Admit only exact `GET` routes for the fixed origin;
   reject user-info, non-default/explicit ports, query, fragment, alternate host
   spellings, frames and every unknown method/route. Every request must receive
   an in-memory response or bounded denial before its deferral completes. Handler
   exceptions, cancellation during an `await`, unsupported CDP methods and late
   callbacks latch failure and cannot be cleared by the later expected-negative
   phase or STOP.

6. **Keep input claims exact.** Use only the frozen CDP command names and schemas
   supported by the pinned SDK/runtime. Keyboard evidence requires browser-
   protocol key events and observable focus/value/form-state transitions; DOM
   assignment, `.click()` and synthetic `dispatchEvent()` are not substitutes.
   Bound the number, request bytes, response bytes and remaining-deadline time of
   every command. AX evidence needs an independently parsed role/name/state
   oracle. Explicitly state that this is protocol input, not physical keyboard,
   screen-reader or visual inspection.

7. **Preserve complete process coverage through the DOM phase.** Keep held handles,
   creation times, exact AppContainer SID, zero capabilities, job membership and
   approved image hashes for every observed identity. Compare cumulative job
   process creation count with distinct verified identities only after final
   termination; a short-lived unopenable process is failure, not a skipped helper.
   Measure renderer-time integrity/restricted-token/mitigation properties on the
   same held identity where supported, without relaxing the exact outer token
   gate. A role supplied by WebView2 does not attest internal sandboxing.

8. **Retain cleanup precedence and make DOM activity cancellable.** The original
   primary exception must remain authoritative while each cleanup step runs.
   Stop accepting new resource/CDP/DOM events before controller close, cancel the
   single bounded DOM operation, close the WebView controller, terminate the
   inner job, query zero while retaining its sole handle, wait for the controller's
   independent `ACTIVE_PROCESS_ZERO`, receive exact `DRAINED`, and only then close
   job/port handles and permit profile deletion. Do not allow an asynchronous
   completion after failure or STOP to emit success. Missing output EOF, held-
   process exit, job notification, acknowledgement or final receipt write remains
   cleanup pending/failure.

9. **Keep this result prototype-only.** The sole browser argument remains
   `--edge-webview-no-dpi-workaround`, a development compatibility control. Every
   input manifest and final receipt must retain explicit prototype and
   unqualified-production markers; host events must retain
   `containment_attested: false` and `browser_qualified: false`. A passing
   DOM experiment proves feasibility of this exact development profile only. It
   cannot promote `webapp-testing` or satisfy production browser qualification
   until the shell-launch issue is resolved without an unqualified development
   flag, or that exact flag is separately accepted as a supported production
   contract with upstream and native evidence.

### Minimal offline test additions

This review list is a target for the complete boundary, not a statement that all
tests below have been implemented. The narrow synthetic diagnostics and their
actual test coverage are recorded in the
[native checkpoint](cs3-native-dom-checkpoint.md). Unimplemented adversarial
cases remain blockers to qualification and untrusted-project execution.

Before qualification, add tests for guardian-worker loss at
each handshake state; nested-job setup rejection; reparse insertion and file-ID
change; entry/depth/time/byte exhaustion; UTF-8 multibyte accounting; chunk gaps,
overlaps, reorder, duplicates and hash mismatch; duplicate/out-of-order phases;
unknown request sources and URI confusables; incomplete deferrals; unsupported or
oversized CDP responses; expected-negative transitions that cannot clear an
earlier fault; STOP/timeout races; late callbacks; and cleanup where any one of
worker exit, held-process drainage, job-zero notification, `DRAINED`, EOF or
receipt persistence is missing.

These fixes preserve the valuable verified startup boundary while keeping the
next run a narrow in-memory DOM feasibility experiment. They add no browser
network capability, HTTP listener, account, firewall rule, runtime ACL change,
download or production qualification claim.

### Guardian and scratch follow-up review

The same-boundary draft now launches its PowerShell worker in a separate,
controller-owned unnamed job through `PROC_THREAD_ATTRIBUTE_JOB_LIST`. The job
uses kill-on-close, forbids breakaway, and caps the worker plus the nested
32-process browser job at 33 active processes. Only the three redirected pipe
handles are inherited. A failed launch terminates the guardian job and requires
its active-process count to reach zero before returning the original failure;
if drainage cannot be established, the combined launch/cleanup failure is
reported. The controller also persists `worker_launch_attempted` before launch
and cannot delete a created profile after an attempted launch unless independent
process-drain evidence exists.

The wrapper's disposal order is guardian termination, active-count drainage,
job-handle close, stream disposal, then process-handle close. A browser-free
native smoke used the fixed helper to start one waiting `pwsh.exe`, pinned its
process identity, disposed the guardian, and independently observed that exact
process exit within the ten-second deadline. The compile-only suite at that
checkpoint passed 31 protocol/coverage, 34 DOM-evidence, and 9 guardian assertions.
This first smoke established explicit disposal, not abrupt owner-loss behavior.
The committed smoke now independently covers explicit disposal and abrupt loss
of the process holding the sole guardian-job handle. It pins the exact worker
identity, requires exit within ten seconds, reacts to parent stdin EOF, and gives
both the fixed waiter and nested owner independent 30-second deadlines. It does
not launch WebView2, create an AppContainer profile, alter an ACL or registry
setting, or inspect browser inputs.

The revised scratch observation holds every ancestor and traversed directory
without write or delete sharing, opens each final component with
`OPEN_REPARSE_POINT`, and checks type/reparse metadata through the held handle.
That closes the reviewed ancestor, replacement, and final-component reparse
races; an existing conflicting writer makes the probe fail conservatively.
An enumerated direct child already pending deletion may disappear before its
attribute hint or handle open. Only `ERROR_FILE_NOT_FOUND` and
`ERROR_PATH_NOT_FOUND` at those two boundaries are skipped, after charging the
entry/time counters; access denial, sharing violations, reparse/type changes,
metadata errors, and recursive enumeration failures remain fatal.
Regular files intentionally retain write sharing while denying delete sharing,
so their recorded sizes can change after observation. The 64-MiB result therefore
remains an observational ceiling, not an immutable quota or storage-containment
claim.
