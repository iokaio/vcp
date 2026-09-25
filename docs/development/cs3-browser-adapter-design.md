# CS-3 browser adapter: bounded implementation design

Status: source investigation and proposed implementation boundary, September 24,
2026. No browser, adapter, profile or service was installed or executed for this
document. This does not complete [CS-3](../plan/24-skills-follow-on.md#cs-3--browser-execution-and-six-skill-acceptance).

## Existing interfaces and their limits

`src/crates/vcp-tools/src/process.rs` provides `Profile::new`, `with_inputs`,
`with_process_count`, `with_max_timeout_ms`, `Request`, `prepare`, `Prepared::pin`
and `Prepared::evidence`. Profiles select an explicit absolute executable, filtered
environment, pinned inputs and bounded execution. A profile digest and source
versions participate in preparation and dispatch. A profile is not authority.

`src/crates/vcp-lifecycle/src/foundation/worker/execution.rs` configures profiles
only during fresh owner setup, checks task/child scope and remaining deadline,
then calls current authority checks. The advertised enforcement set is JobTree,
ProcessCount, FilteredEnvironment, Timeout and OutputLimit, with PTY conditional
on availability. It does not advertise network or workspace filesystem isolation.
Without explicit reduced isolation, process preparation additionally requires
WorkspaceFilesystem and NoNetwork; unavailable required controls must keep denying
dispatch. Browser work must not change that enforcement report to gain admission.

`src/crates/vcp-lifecycle/src/process.rs` and `process/launch.rs` own suspended
launch, Job Object assignment, descendant limits, output observers, timeout and
termination. `Process::wait` drains the owned job; dropping a process attempts job
termination. `process/duplex.rs` provides the existing bounded interactive process
connection if a later concrete protocol requires one. Initial browser checks can
finish within one ordinary foreground execution instead.

`src/packages/vscode/tests/inspector-cdp.cjs` demonstrates real renderer assertions
using Node's WebSocket support and a private `DevToolsActivePort` file. It bounds
that discovery file to 4096 bytes, validates its port/browser endpoint, uses
five-second command deadlines and tracks execution contexts. It attaches all page
and iframe targets and permits arbitrary test expressions. This is a test helper
for an owned editor, not a browser security adapter. It has no origin allowlist,
request interception, download policy or comprehensive target/network cleanup.
Reuse its approach to independent DOM observations, not its authority assumptions.

## Smallest compatible construction

Begin with an original, finite browser-check launcher selected by an explicit
process profile. Run the launcher, owned application server when needed, browser
and its descendants in the existing Job Object. One foreground check avoids a
second persistent service or model-controlled browser daemon. Pin the launcher,
check specification and relevant fixture inputs through the existing profile
mechanism. Browser and server executable identity must also be verified and bound
before launch; merely naming an installed executable in the check file is not
equivalent to the broker's executable pinning.

Use an already provisioned Chromium-family browser only after its exact build and
protocol behavior are recorded. No particular browser version or CDP capability is
qualified by this proposal. Read primary documentation for that build before
implementing target attachment, request blocking or input events. Provisioning
browser binaries remains a separate authorized action.

The first check specification should declare one literal loopback origin, expected
application identity/readiness, an owned-server launch or an explicitly reused
server, a fresh output/profile directory, finite actions/assertions and limits.
Reject unknown fields and caller-supplied executable flags. Use exact parsed
scheme/host/port matching, not string prefixes, wildcard hosts or suffix checks.
Avoid hostname resolution in the initial fixture by using a literal loopback
address. Reject URL credentials and unsupported schemes. A redirect must be
checked before following it; observing a forbidden final URL is too late.

Create a fresh private browser profile beneath a new owned directory. Never accept
a user's normal profile or an arbitrary remote debugging endpoint. Bind debugger
discovery to that directory and launched browser; stale discovery files must fail.
Keep debugging private to the owned run. Do not add sandbox-disabling flags. Avoid
logging page contents, cookies, authorization fields or raw protocol messages by
default; retain only bounded assertions and redacted failure context.

Start an owned server only through the declared profile/launcher path and wait for
a bounded application-specific readiness predicate. A port accepting connections
does not identify the intended application. Long polling does not block readiness
when the predicate is satisfied. An occupied port fails explicitly; do not kill
its owner or silently attach. Reused servers are never included in owned cleanup.

## Network policy is the outstanding qualification boundary

Job Objects do not restrict network traffic. A browser's request interception can
be part of an application policy but is not evidence of host-wide confinement.
Do not claim arbitrary hostile-page isolation or NoNetwork from a CDP filter.

Before advertising scoped browsing, independently qualify blocking before network
dispatch across navigations, subresources, redirects, frames, new tabs/popups,
workers, service workers, WebSockets, downloads and non-HTTP schemes applicable to
the selected build. Background browser traffic also requires an explicit scope
and evidence. New targets must be held before executing page content until policy
is attached. Policy setup failure or debugger disconnect must stop the run, not
let the browser continue unsupervised. Unsupported channels must be disabled with
verified behavior or make that workload unsupported.

A local proxy alone does not prove enforcement: direct connections and alternate
protocols may bypass it. If the selected browser cannot enforce the advertised
scope, implement a separately reviewed host enforcement boundary or retain the
support gap. Do not weaken required policy or silently use reduced isolation to
claim equivalent protection. Reviewed synthetic, offline-content fixture trials
can establish individual controls under explicitly authorized process access;
they cannot establish general network confinement or close the full CS-3 gate.

## Lifecycle and evidence

The canonical task owner remains responsible for cancellation. Use existing
timeout, output and descendant limits for the entire launcher tree. Normal
completion closes browser/server connections and verifies child exit before
reporting success. Pause, cancellation and owner loss terminate owned processes
through the broker; a JavaScript `finally` is insufficient when the owner is killed.
Retain failed or unknown cleanup outcomes. Clean temporary profile/output files
only after process termination is observed, using exact owned paths and expected
identities; never recursively delete a user-supplied path.

Bind observations to source/fixture digests, profile/launcher/browser identities,
the allowed origin, action sequence and actual checks. Preserve raw canonical
process outcomes separately from interpreted assertions. Changed source requires
new evidence. DOM/layout measurements do not qualify model visual inspection.

## Proposed implementation files and tests

These are proposed files, not current adapters:

- `scripts/evals/cs-browser-launcher.cjs`: finite specification validation, owned
  browser/server coordination, qualified protocol client and bounded result file.
- `scripts/evals/cs-browser-launcher.test.cjs`: deterministic URL/scope validation,
  stale discovery, bounded queues/results and protocol failure handling. Protocol
  fakes test launcher logic only, never browser/network enforcement.
- `src/evals/skills/follow-on/web/`: original application fixtures and independent
  expected interaction/effect records, bound by a separate CS manifest. Preserve
  the frozen P7 fixture inventory.
- `src/crates/vcp-cli/tests/local_cs_browser.rs`: actual native browser checks
  through a configured process profile, with isolated output/profile directories
  and exact executable/source identities. Follow existing native qualification
  gating conventions without hiding not-run cases.

No new core browser tool is necessary for the finite first increment. Extend
production interfaces only if implementation demonstrates an unmet pinning or
lifecycle requirement; keep such changes at the existing owning boundary with
their own tests. Do not promote the VS Code test helper into a production broker.

Actual Windows qualification must cover: a useful form and keyboard/focus path;
loading/error states and long polling; missing browser; startup failure; occupied
port and user-server survival; allowed and denied redirects; frames/workers/popups
and alternate network channels; secret-free evidence; stalled protocol messages;
source changes after a pass; timeout/output/descendant limits; pause/cancel; owner
close and abrupt owner loss. Independent local listeners must observe whether a
denied request arrived, rather than trusting the browser's own reported outcome.

Reuse lifecycle regression evidence from
`src/crates/vcp-lifecycle/tests/duplex_process.rs`, especially
`owner_close_and_drop_drain_without_callers_waiting`,
`pause_rejects_further_io_and_cancelled_partial_write_kills_connection` and
`cumulative_input_and_job_descendant_limits_are_observed`. Add actual browser-tree
observations; existing generic process tests cannot establish browser cleanup.
Record locked-profile and Unicode/space/long-path behavior, all missing prerequisites
and every failed attempt. No CS-3 executable-support claim is justified until the
applicable policy and lifecycle matrix passes on the selected native toolchain.
