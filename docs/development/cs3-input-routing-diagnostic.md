# CS-3 input-routing diagnostic

September 27, 2026. Diagnostic evidence only; CS-3 and browser qualification
remain open. This follows the [earlier DOM checkpoint](cs3-native-dom-checkpoint.md)
without replacing any failed receipt.

## Finding

The same invisible, message-only WebView can receive CDP text and keyboard input
in the existing remote session and zero-capability AppContainer. Changing RDP,
showing a desktop window, granting network capability or provisioning another
machine was not necessary to demonstrate it.

Controlled timing variants isolate a **pre-dispatch readiness dependency**:
delaying before Enter permits delivery; delaying only the observation afterward
does not recover the events. This supports an early-input suppression/readiness
race, not a late DOM snapshot alone. The precise internal WebView2 gate is not
identified, and the result is not a general guarantee about RDP or WebView2.

All completed routing runs used the same exact runtime/SDK/loader, embedded page,
CDP target query, focus emulation, native `MoveFocus(Next)`, Enter parameters and
containment. Both successful and unsuccessful timing variants reported native
focus (`GotFocus=2`, `LostFocus=1`, a nonzero focus-window PID) and the expected
DOM active element. Neither signal alone establishes keyboard readiness.

## Controlled results

Each pre-key delay is applied twice: after native focus, then after the optional
text-insertion stage. The no-insertion variant leaves that stage's value empty.
Zero milliseconds means no asynchronous delay at that point. All numbers below
are observations, not qualification thresholds or retry policy.

| Build / run | Insert before Enter | Pre-key delay, each | Post-key delay | Result |
|---|---|---:|---:|---|
| 14 / `36405dd017b743efbca497cd2934a84a` | planned | 100 ms | 100 ms | Stopped before routing: scratch inspection access denied, followed by parent process-identity lookup failure. Retained, not replaced. |
| 14 / `e0452c6fecd24c88a0b9cc1d3d4f5415` | Ada | 100 ms | 100 ms | Correct CDP target; inserted Ada; one keydown/keypress/keyup and submit; `Saved Ada.` |
| 15 / `cdfebda66a254ff28260434c4baeddfa` | none | 100 ms | 100 ms | One keydown/keypress/keyup and submit; `Name is required.` |
| 16 / `123b82d59b404d3e8a754c623f96d461` | none | 0 ms | 0 ms | Correct target and native/DOM focus, but zero key or submit events. |
| 17 / `8d8abd4c926c4ace858502d5b99cb861` | none | 0 ms | 100 ms | Still zero key or submit events after waiting. |
| 18 / `73db5955ccbd4c00884a0e6cd8a9100a` | none | 100 ms | 0 ms | One keydown/keypress/keyup and submit; `Name is required.` immediately observable. |

The two build-14 attempts used the identical frozen manifest. The first failed
before answering the input question; its access check was not relaxed. It has
3/3 worker-verified process identities; the other five runs have 7/7. All six
drained independently, removed only their newly owned profiles, and passed
runtime/host/policy before/after checks. Profile directories were independently
checked absent. All made zero provider calls. Existing unresolved profiles were
untouched. No visible-window experiment was needed.

## Exact retained identities

Raw receipts and staged inputs remain under ignored `artifacts/cs3-dom-build-N/`.
Each receipt is `run-<identity>/native-receipt.json` within that build.

| Build | Input manifest SHA-256 |
|---|---|
| 14 | `f76eb3c04949657af0d83f094a27e61d91034e0db6357776e11bd815b1a621af` |
| 15 | `b44fee6148327a8c4828ba2bd4de9c02ff0fc13bee59bf359a211d7648c53438` |
| 16 | `73e5e9b8a02bed96bb9a89dcaed7f70a5f4d10cc2cd24076ab482de79aa4bab6` |
| 17 | `8565b1a115f9b048da72392ebab18015f11f2f908de7318871bc142d7379a311` |
| 18 | `5aa5dc6b640cf79e97377467825dfc4c21ef444f7bcd67f5aecb31b17c839154` |

| Run | Receipt SHA-256 |
|---|---|
| `36405dd017b743efbca497cd2934a84a` | `c48d8f6fd9849856fb098be60ef53938f779362b10dad9ab8a016e92b4656a87` |
| `e0452c6fecd24c88a0b9cc1d3d4f5415` | `ebe433ce9eb6bd46b089dab2434c2ad01072d1c5ffebff454622c66757587ef3` |
| `cdfebda66a254ff28260434c4baeddfa` | `2cf58f832172ecbc903ec17e4e7ccf87b35bbb593cb5e1a8c98eff8ff9afe710` |
| `123b82d59b404d3e8a754c623f96d461` | `a6e7d2d33f164a5a5fd5a15a5abe517a2879c613a2e9c35446a16d8640e092f1` |
| `8d8abd4c926c4ace858502d5b99cb861` | `e811a2b15216339634c470e5461bcc6b75f50b6f00fe3db0fe127a2264a63f39` |
| `73db5955ccbd4c00884a0e6cd8a9100a` | `7da8f5392e09fc8460eefec8abf51c3cfbd481c3598d09a6e784ea5af2182bf8` |

The first build-14 receipt labels the run `input_diagnostic_inconclusive_or_failure`
and retains `primary_controller_failure`; its process exited nonzero. Subsequent
source restores the distinct `controller_failure` label for that path. The raw
receipt is unchanged. Parent focus-PID validation also changed after build 14
to retain PID zero as a typed observation, not infer ownership. These parent
changes do not alter browser dispatch. Builds 15–18 share the same parent code.

## Implementation and verification

The fixed routing command contract permits only a read-only `Runtime.evaluate`
identity query, focus emulation, optional literal Ada insertion, and Enter down/up.
There is no caller-supplied script, command, origin, project or timing option.
The current source is the no-insertion, two pre-key 100-ms delays, zero post-key
delay control. Frozen builder metadata records the variant; it is not a product
configuration or production workaround.

The parent reconstructs four ordered, hashed, bounded documents (`target`,
`focus`, `text`, `key`). Wrong page identity, malformed/duplicate fields, corrupt
or out-of-order chunks, byte ceilings and regressing counters fail closed.
Actual input results are observations: `input_diagnostic_observed` can accompany
`key_delivered=false`. This route cannot satisfy the separate full DOM oracle,
and all browser/production qualification markers remain false. Late profile
cleanup errors invalidate an observed outcome.

Builds 15–18 each passed 71 host contract, 198 fixture, 33 policy/identity/PE,
46 protocol/coverage, 47 DOM, 25 routing, 9 guardian and 7 scratch assertions.
Native failure/cleanup evidence is retained separately from those pure checks.

## Interpretation and next implementation gate

Chromium's input injector can return success when no keyboard event was queued;
browser/embedder checks can reject the event before renderer delivery. That is a
mechanism consistent with these observations, not proof of the exact proprietary
WebView2 implementation path. See [input injector source](https://github.com/chromium/chromium/blob/main/content/browser/devtools/protocol/input_handler.cc)
and [keyboard forwarding source](https://github.com/chromium/chromium/blob/main/content/browser/renderer_host/render_widget_host_impl.cc).

Do not turn the diagnostic sleeps into an assumed readiness contract. Next work
is a bounded, evidence-backed input-readiness strategy and repeated native
validation, followed by the unchanged full form/AX/origin/lifecycle acceptance.
Do not retry an action blindly or replace keyboard input with a synthetic DOM
submission to make the oracle pass. The development flag, serviced-runtime,
internal sandbox, network boundary, owner-loss qualification and six-skill
acceptance limitations remain open. A separate worker may still help isolation
qualification, but is no longer a prerequisite conclusion drawn from this input
failure.
