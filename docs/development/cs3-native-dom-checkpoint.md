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

Attempts through the September 27 F24 run used the development-only
`--edge-webview-no-dpi-workaround` argument. The later production-profile run
recorded below removed it. Before/after Evergreen runtime hashes are not
continuous immutability proof. Exact outer AppContainer identity, capabilities
and job coverage are not proof of Chromium's internal renderer sandbox.

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

## September 27 Hyper-V F24 full-DOM attempt

The later disposable-VM attempt built source commit `979d500` against WebView2
`153.0.4234.48` and SDK `1.0.4191.47`. Its build receipt SHA-256 is
`ac3ac06df8d0231fcc8ceb3227b48fc920854058014dad1f9db7df693667d31b`;
the embedded input-manifest SHA-256 is
`7c035d48e5b07efe1f310cb76c43f6d3188b1a17ea94457faaa5a9143c1d4f70`.
The launcher ran once as local non-elevated `User` in active VMConnect Basic
Session 1. The controller was not run through PowerShell Direct: a one-shot
limited-token interactive task started the already staged, hash-pinned launcher
in that console session and was removed after it returned.

The retained run identity is `f9690ec599c04ff98775b5912c256720`.
The 579000-byte host collection report SHA-256 is
`2b948fc3011874e2b6fdee50a08b9f3c41854e1aabe11af09c4be4324169956f`;
its embedded native receipt SHA-256 is
`f0f8f8f6d958adb347654782d55f06249f4e006ed316c9cddadb92169c06127f`
and launcher-result SHA-256 is
`f9e4cc5f2670ef2360fda32995973f54f77d059987613a994e733b930f8391fc`.
Collection was read-only and did not launch or retry the browser.

The F24 readiness strategy resolved the earlier input-routing failure. Five
ordered, independently reconstructed and hash-checked documents establish:

* native forward focus moved `BODY` to `name` after one trusted F24 down/up
  pair with no keypress or repeat;
* the first exact Enter produced one trusted keydown/keypress/keyup and submit,
  preserving the empty value and yielding `Name is required.`;
* one literal `Ada` insertion followed by a second trusted F24 down/up pair
  produced `value=Ada` without another submit; and
* the second exact Enter produced the second trusted keydown/keypress/keyup and
  submit and yielded `Saved Ada.`.

No F24, Enter or text action was retried. The five document SHA-256 values are,
in order, `0e5de04e060f1d102d4a0bd50c0a017b5737a016a561d5b934abbf368de257e1`,
`1223572b5906a9c44e6598e7bc2b434a8488797d8a55fb6a7379bab61ad476bc`,
`f519689da986b8cc9bad08951ea4177337c21d7f43df4bafbfdcddabb68f9842`,
`859b987cfde08264d05f4dce33636a164212d6d1ad28f55d524b13c6cec88a34`
and `4d86311be6cf80de78d0fe4fc7a9db31ed014b5d17510dc88e6c346214657380`.

The attempt then failed closed in `accessibility_snapshot` with
`0x80131501` (`InvalidDataException`) before accessibility evidence or the
negative-origin phase was accepted. The current contract applies the ordinary
8192-byte DOM-record ceiling to the raw `Accessibility.getFullAXTree` response;
the small synthetic form's verbose full-tree response is the leading evidenced
cause, although this receipt does not retain the rejected response length and
therefore does not prove which accessibility assertion threw. The next change
must keep the raw protocol response bounded while emitting a compact, independently
validated accessibility projection; merely removing the ceiling is not acceptable.

Cleanup still completed: process coverage was 7/7, the independent job-zero
acknowledgement completed, all processes drained, the profile was removed, and
runtime, host and policy postchecks were unchanged. There were no cleanup errors,
stderr bytes or queued diagnostic lines. The native outcome remains
`input_diagnostic_inconclusive_or_failure`; accessibility, origin, owned-server
and browser-qualification gates remain open.

## September 27 automated production-profile DOM evidence

PowerShell Direct was used only to stage hash-checked files, register and observe
a one-shot limited-token task, and collect receipts. The browser controller ran
as local non-elevated `User` in active VMConnect Basic Session 1. Every task was
unregistered after it returned. The VM network adapter remained disconnected;
no paid provider call or retry of an existing build occurred.

The final source commit was `222cc797a2f208901f6302b3fcaff793c5a1b598`.
Its build input-manifest SHA-256 was
`a423c7b33b3264d74f96cfc6bd1edbc667337be2c41cfd1f4518f23a4d27a8af`.
The manifest and host contract require an empty additional-browser-argument
value. Run `c715652e31a442dd86dd09ba0700ac54` produced native receipt SHA-256
`ceecc8fef324b13f409a3169747d69b4795a3ab1c98ed1f2799a5a550ca48db6`
and launcher-result SHA-256
`57a8725dc85c0e2520f223d04ae418e1f2d21d7aa1c18aa9cef4b2855c35d9ff`.

The authoritative native outcome is `dom_observed`. Eight cumulative job
processes had eight independently verified identities; all were zero-capability
members of the exact disposable AppContainer and owned job. Independent
active-process-zero acknowledgement completed, all held identities drained,
the disposable profile was removed, runtime/host/policy postchecks were
unchanged, and there were zero rejected observations, stderr bytes or cleanup
errors.

Seven independently reconstructed documents passed the parent oracle:

| Document | Bytes | SHA-256 |
|---|---:|---|
| `initial` | 409 | `0e5de04e060f1d102d4a0bd50c0a017b5737a016a561d5b934abbf368de257e1` |
| `focused` | 411 | `1223572b5906a9c44e6598e7bc2b434a8488797d8a55fb6a7379bab61ad476bc` |
| `invalid` | 433 | `f519689da986b8cc9bad08951ea4177337c21d7f43df4bafbfdcddabb68f9842` |
| `filled` | 421 | `859b987cfde08264d05f4dce33636a164212d6d1ad28f55d524b13c6cec88a34` |
| `success` | 431 | `4d86311be6cf80de78d0fe4fc7a9db31ed014b5d17510dc88e6c346214657380` |
| `accessibility` | 607 | `374f782d9248d23dff18d656fdeb1649da759dfa8591ff687c992810f212bf53` |
| `origin` | 86 | `12fe676287b999fbf42912d23ab5b96be4cb236f4ff65e3d7b22c0b43a30a6bb` |

The accessibility projection identity-binds the bounded 10,219-byte, 23-node
raw `Accessibility.getFullAXTree` result with SHA-256
`3bbee527eee4a423ec32f92644871f7cdb8888a28b2580645d57e4cf144bdbcd`.
It contains the exact document, textbox value/required state, button, alert and
status roles without weakening the ordinary 8 KiB DOM evidence ceiling.

This closes the production-profile synthetic form/accessibility/origin and exact
process-accounting evidence gap. It does not claim the separately required
owned HTTP-server join, adversarial network/filesystem boundary, cancellation/
pause/owner-loss matrix, retained UI regrade, model visual review, paid
comparison or six-skill distribution acceptance. The emitted event therefore
continues to report `browser_qualification=false` and `prototype_only=true`.

## September 27 isolated Windows owned-server evidence

The bounded server and its contract tests were staged into a fresh guest path
with portable Node `24.21.0`; nothing was installed and the guest PATH, registry
and network configuration were unchanged. The Node executable was 93,580,104
bytes with SHA-256
`ba4e6d110e8c1592a1ecd390f6b05f3da124b13871a5be62b341a07a853c6c32`.
The server SHA-256 was
`c8df371e3958ed5f87a0a461c133d422db393e536c3cc5c74c93e0afffab7c7e`
and the test SHA-256 was
`93c1838b5ab471ea1a3fdad769d7e8018e2178ed5bd860d02e90195f50f2b0c1`.

With zero connected guest network adapters, all ten actual Windows server tests
passed. They covered immutable hash-checked serving, changed/oversized/linked/
aliased/traversal rejection, forged-inventory rejection, exact host/method/raw-
path/credential/body boundaries, unrelated-listener preservation, incomplete-
client and delayed-response shutdown, request and connection-attempt ceilings,
max-connection drops, and abrupt owner-process loss. The owner-loss test proved
that only the owned ephemeral listener closed and an unrelated listener survived.

This establishes the server lifecycle independently. It does not join the
zero-network-capability browser to loopback. A future join must not silently add
a broad loopback exemption or adopt an existing user-owned service.
