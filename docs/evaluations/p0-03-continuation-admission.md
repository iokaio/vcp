# P0-03 continuation admission increment

Status: native admission regressions pass locally; complete pause/recovery remains
unimplemented. Remote CI is recorded on the associated PR before merge. This is
partial evidence for P0-03, whose ledger state remains `in_progress`.

The [implementation guide](../development/continuation-admission.md) maps the five
changed imported files and remaining lifecycle obligations. The source revision
is Codex `3d3ae4965ab370217e871b3a7f0d15589557ee4b` plus the ordered six-patch series;
no Cargo dependency or lockfile changes are included.

## Observed behavior

| Case | Independent outcome |
|---|---|
| Delegated input, ordinary admission closed/open | Denied continuation records no request; explicitly reopening admits one request containing only the accepted input |
| Review delegate, ordinary admission closed/open | Continuation hook observes denial, review exits with no output, provider records zero requests |
| Two mailbox messages behind closed admission | Both wakeups are denied; explicit reopening/wakeup produces one request containing all retained messages and excluding the rejected ordinary start |
| Original host-drain compatibility | All five upstream drain cases still pass, including child input, mailbox work, review completion and realtime shutdown |

The provider is loopback and uses synthetic dummy authentication from the retained
test support. No OpenRouter spend or real workspace transcript is involved. These
tests verify the gate and input retention, not active-worker cancellation or
durable task state.

## Native baseline and limitations

Windows x64, Rust 1.95.0, MSVC 14.50.35717, four build jobs. The retained
`codex-core --test all` target built from the workspace with native flags. Five
original drain tests passed before modification; the changed controller passed
those five plus five new continuation tests. The first baseline runtime attempt
overflowed a default libtest worker stack. An explicit 8 MiB test-worker stack
made those tests pass; no product runtime-stack fix is claimed.

The unchanged PTY library was also sampled during preparation: 21 of 24 tests
passed. Two failures selected the Windows Python app alias; both exact cases
passed when the test subprocess instead used the provisioned Python interpreter.
`tests::windows_tests::conpty_ctrl_c_interrupts_powershell_foreground_child`
still failed alone: the foreground loopback ping did not yield the expected
subsequent console marker after Ctrl+C within ten seconds. This remains a
pre-existing failure, outside the continuation gate's claimed coverage. The strict
contained-process and descendant-termination cases passed. This does not qualify
the whole PTY suite or Windows close-to-pause behavior.

Review identified that ordinary-open/review-closed admission must be tested
separately; the final implementation always consults the continuation hook for
review delegates. Permits admitted before sealing can still start work. A real
pause host must track/cancel that work and await durable reconciliation before
acknowledging pause. No current default host seals this new hook.

## Reproduction evidence

On September 18, 2026, the complete local wrapper passed from 08:14:58 to
08:17:28 UTC, using `-TargetRoot artifacts/upstream/codex-target` to reuse the
qualified native build cache. The native observer verified ten tests, exit 0.
The test binary SHA-256 was
`39f775990c790541f7015e01c6285be5f6f4c2e5abc225a03be633bfa3526452`.
The local result lives under
`artifacts/build/dbc6c60f-56cd-4159-a419-9c40bdcc2622/`; raw artifacts are ignored.

Independent reconstruction produced 7,937 files and result inventory digest
`dabcf15ddff6228a40ebbeb774b3c42cf8245604245e61b603aacd8c120d4456`.
`scripts/test.ps1 -Suite fast` passed all eight cases at 08:17:26 UTC, including
three new observer regressions. The repository checker found 120 Markdown files,
1,236 links and unchanged 68-task/56-release-task ownership, with no errors;
`git diff --check` passed. Final wrapper evidence also hashes Cargo stderr.

Reproduce through `pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests`.
The wrapper records exact commands, compiler, binary hash, source identities,
ten required test results and bounded captured output in ignored evidence.
The fast suite also tests rejection of false-success native logs and ambiguous
Cargo artifacts. Independent reconstruction checks the source and ordered patch
series. Local passing results do not establish remote CI or completion of P0-03.
