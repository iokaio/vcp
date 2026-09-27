# CS-3 synthetic DOM diagnostic checkpoint

Status: diagnostic work in progress, not CS-3 completion or browser qualification.
Later [input-routing diagnostics](cs3-input-routing-diagnostic.md) demonstrate
keyboard delivery in the same hidden host and isolate a pre-dispatch timing
dependency. They supersede the unresolved-input/environment recommendation below,
not the retained failures or outstanding qualification gates.
See the [phases](cs3-implementation-phases.md),
[boundary review](cs3-account-boundary-review.md), and
[six-skill readiness ledger](cs3-six-skill-readiness.md).

## Scope and preservation

The new source under `src/tests/support/windows/webapp/` exercises one original,
fixed in-memory form inside a fresh zero-capability AppContainer. It accepts no
project path, network origin, arbitrary script or CDP command. Runtime, SDK,
loader, source and toolchain identities are recorded by
`scripts/evals/webapp-browser-build.ps1`; every native attempt requires the exact
reviewed build-manifest hash. Builds and private receipts stay under ignored
`artifacts/`, not in the product catalog or native package.

No account, firewall, Windows feature, runtime permission or existing profile was
changed. Each attempt created one uniquely named disposable AppContainer profile
and restricted only its newly owned directories. Its profile was removed only
after independently observed process drainage. All eleven attempts below have
`status=cleaned`, `processes_drained=true`, unchanged runtime/host/policy checks,
zero provider calls, and a separately checked absent profile directory. The
failed build `cs3-dom-build-01` and compile-only build `cs3-dom-build-02` are also
retained. The two older unresolved profiles and unrelated worktrees were not
reconciled or modified.

## Retained attempts

These are separate diagnostic attempts, not campaign retries or replacement
results. No failed observation was promoted after its source was corrected.

| Build / run | Observed result | Coverage |
|---|---|---|
| 03 / `44ea4d91e419455cb2f92381913aed17` | Initial DOM loaded; host failed before accepted invalid-form evidence. Failure-line retention needed correction. | 7/7 |
| 04 / `a7f45629f7ca4baaa13781cf439db541` | Live scratch observation failed when an enumerated browser temporary file disappeared; controller subsequently could not open an already exited child. | 5/6, incomplete |
| 05 / `685ae78dfec3486682ddaf49715bce42` | Exact equality between separate live PID/accounting snapshots raced new process creation; controller subsequently lost a child identity. | 2/3, incomplete |
| 06 / `9546f86c5fb04da69894bd316a1ab957` | Parent rejected the invalid-form snapshot: it was identical to initial state, including `active=BODY` and empty error. No successful keyboard/form claim. | 7/7 |
| 07 / `351134bf9a8e400391d3bbec7681f3a3` | Added fixed focus emulation, exact command/response sequence and late-callback guards; invalid state remained identical to initial. | 7/7 |
| 08 / `d7aa908db2d049b08b75977b0caccfab` | Enabled an 800x600 rendering widget under the message-only parent; same invalid-state rejection. | 7/7 |
| 09 / `733364fda1374b95b03b5e815a236a87` | Added native programmatic WebView focus entry; same invalid-state rejection. | 7/7 |
| 10 / `8bdff10efb2243e19b84e502efa89bb9` | Used an owned hidden Form parent, never shown or activated; same invalid-state rejection. | 7/7 |
| 11 / `351a91ec35e2491c8a1b369180fe9b5a` | Used non-text `rawKeyDown` for Tab and omitted optional native-key hints; same invalid-state rejection. | 7/7 |
| 12 / `c80f1ae955754078922437e1aa596dd2` | Native forward traversal independently moved `BODY` to `name`; CDP Enter still produced no validation message. This establishes native focus movement only, not CDP Tab or submission. | 7/7 |
| 13 / `70eb14b651ef4b9d83694e6f223cc6b4` | Passive diagnostics confirmed script readiness and focus on `name`, but zero keydown, keypress, keyup or submit events after the awaited CDP Enter calls. The page did not receive the requested input. | 7/7 |

| Build | Input-manifest SHA-256 | Private receipt SHA-256 |
|---|---|---|
| 03 | `e3dfd4fede4d194c5d8dcf4434b4b22c2b4aefffa038b3340a5f62b4546a961f` | `97ec19ae02b77b9f7a9ec0aeaba724fb8d7659e0dc852ed88c3e592cec848e10` |
| 04 | `e47ee62a2386656662f8bdec9f91ef1da0a229b7dd036dd278c927aa195ea9ff` | `900bd03b4e77647b6d8e2c76ae0463bc8af8614dbc7330c302d5a8f953039fd1` |
| 05 | `fbfed61660721860cd6512c026c0baa1cc820349398868ff400c64e569ba95df` | `c215e639ecdd50f8c8f449f7a4700692fd04188f23ad1b52585a8dfc04d54bb5` |
| 06 | `7556f22e97c751762b7f3b7016cd65f7ff993e42c689292fff4b72d576dedd07` | `efd4af476e9443efcefcc2ffc92b028c0fd205d1a25debb2a50b89c77a4b264a` |
| 07 | `50f4388338e8663b5c2e573aec6555ee78eac55e9e1cb412c0f19b7b90b0f304` | `ef4c9d478169c84b35f07292468f55aef750c76cd0be7f6d57e27681c88fb9fb` |
| 08 | `c87ebd1649a83ebad4234279b93b4ae248df8ca3408b1d0879baea6ac61b2c76` | `16ada1148fae05e823f9326d93c4a0d92da6fde9ae17c8d520a4d115908698fd` |
| 09 | `9f198a0e935e21f508e34bc464fe00fd40eaefb0f386186bf6dc89c29146cb86` | `410824410b8826f1377b089af4e2572652ed4c14947a18ed0f03d9a2daafdbc2` |
| 10 | `1e39d674a1d6dcd1e886e039dffefa1ca4b34477924e15df30bd317067c60853` | `84a906c1e8a2e7a47e76ed739cf49ad8fc44ccf7fa3738986beb46e96d3ee768` |
| 11 | `363d97d5e66d1670b5053307844a752536279a34e59a1d17691765ef4df7774f` | `13d049e27365e62ec0c9b3d1104a28dfee38e08484854c57fc79ca0de2864bfb` |
| 12 | `b4e6624476bb4f26b39582b270d214fe0bdf8e733c45b5a846f8253bde8d12f2` | `754c5aa88a4cdad24373b87981ba267cec44f7f013352cd1f355b544bd014aa0` |
| 13 | `bdf3b39671b8ceb26ac53732c188f340b5e066e16359d942814e6bafb85ba3bc` | `4abd76ade80493077353dcb3c645bc4c2b3176ac87d65350b8b25853a7e3b05e` |

The scratch fix skips only vanished direct children at the hint/open boundary;
access, sharing, reparse, type, metadata and traversal errors remain fatal.
Live cumulative accounting may exceed the immediately preceding PID snapshot.
Final accounting after termination still requires exact cumulative/verified
equality: missed short-lived processes remain a failed attempt.

For these diagnostic invocations, PowerShell exit zero means receipt/cleanup
completion, **not** browser success. The receipt's outcome is authoritative.
No accepted invalid/valid form transition, accessibility tree or negative-origin
result was reached. Native forward traversal was observed in builds 12 and 13.
Build 13 rules out an unregistered fixture handler at the observation point:
`scriptReady=true`, `active=name`, all four event counters zero, empty `lastKey`,
and empty validation text. These observations do not identify the internal
reason for dropped input or establish a general WebView2 defect. No synthetic
DOM click, submit or keyboard event was substituted for the failed protocol input.

The bounded hypotheses used the documented
[WebView visibility](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.isvisible),
[host focus entry](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.movefocus),
and [CDP focus emulation](https://raw.githubusercontent.com/ChromeDevTools/devtools-protocol/master/pdl/domains/Emulation.pdl)
interfaces. [Playwright's Chromium input implementation](https://raw.githubusercontent.com/microsoft/playwright/main/packages/playwright-core/src/server/chromium/crInput.ts)
was consulted for non-text-key protocol semantics, not imported as an execution
dependency. No visible desktop window or foreground-activation call was used.

## Offline verification

The fresh build passed 58 host lifecycle/protocol assertions, 198 fixture
assertions, 33 policy/ACL-rule/identity/PE assertions, 41 parent protocol/coverage
assertions, 47 independent DOM/AX assertions, 9 guardian contract assertions and
7 vanished-scratch-entry assertions. These are not a browser pass.

`Test-WorkerGuardian.ps1` independently passed explicit disposal and abrupt
sole-owner-loss smoke checks using only fixed, bounded waiting processes.
The candidate, HTTP server and WEB preparation loader passed 35 Node tests,
including listener owner loss, unrelated-listener preservation, request and
connection ceilings (including saturated-listener drops), pinned manifest and
file bytes, redirected paths and candidate/oracle
separation. The WEB manifest SHA-256 is
`dbe34a441187381f0e5d3f587ea72b0ac15e096ca9d90f99a065ef8e51d84ee8`.

The repository fast suite passed all 22 groups, including retained CS-1/CS-2
contracts and the new CS-3 group. Its retained run manifest is
`artifacts/tests/3ef52000-5006-4925-95a3-baaea555f03b/manifest.json`.
Repository validation also passed with zero errors. None of these offline gates
changes the failed browser outcomes.

## Interpretation limits

The selected WebView2 diagnostic still uses the development-only
`--edge-webview-no-dpi-workaround` argument. It is not a supported production
profile. Before/after Evergreen runtime hashes are not continuous immutability
proof. Exact outer AppContainer identity, capabilities and job coverage are not
proof of Chromium's internal renderer sandbox.

Protocol keyboard/text input and emulated active-page state, if demonstrated,
are not physical keyboard input, a screen-reader test or human visual review.
The intended AX assertions cover document/control roles, textbox name/value/
required state and button name, not live-region text relationships. The scratch
byte ceiling is an observation, not a disk quota.

The separately tested owned HTTP server serves only immutable synthetic input
bytes on its own ephemeral loopback endpoint. It has not been joined to this
browser, which receives no network capability. Static WEB cohort/oracle files
are preparation data, not an implemented browser oracle or an authorized model
campaign. Full origin/network adversarial tests, browser cancellation/pause/
owner-loss qualification, retained-UI regrading, six successful comparisons and
exact native distribution acceptance remain open.

## Handoff and next decision

Retain this increment as a draft review PR, not a completion PR. No default
catalog, product execution adapter, existing account or firewall policy changed.
The current host has no available administrative token; account provisioning
would not correct the rejected isolation assumptions in any event.

The recommended next environment is a disposable, isolated Windows qualification
worker with an explicitly reviewed runtime, filesystem and network policy. Its
availability and permitted provisioning need owner input. That recommendation
is not a claim that a VM automatically satisfies the boundary: it must still
prove the phase-1 adversarial and recovery gates before untrusted project runs.
The current AppContainer input-routing failure remains retained, not waived.
No further speculative runtime flags, host-policy changes, elevated commands,
paid runs or production integration are part of this checkpoint.
