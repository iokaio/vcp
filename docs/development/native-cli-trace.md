# Native Codex CLI trace

This P0-07 experiment executes the imported Codex CLI against an in-process
loopback Responses fixture. It observes the retained request/tool/completion
path independently of the CLI's reported success. It does not implement VCP's
OpenRouter gateway, canonical store, accounting, pause or recovery.

## Run

Build the executable through [the native source procedure](codex-source.md),
then pass its actual output path explicitly:

```powershell
npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund
node scripts/upstream/trace-cli.cjs --binary artifacts/upstream/codex-target/x86_64-pc-windows-msvc/debug/codex.exe
```

The runner requires native Windows and Node 24.10.0. It verifies the committed
source inventory, records the supplied binary's SHA-256 and records the VCP
commit/dirty identity, helper/runner hashes, command lines and process results.
The executable hash identifies the tested binary; it does not attest that an
arbitrary caller-supplied executable was built from those source inputs. Keep
its build manifest with the trace evidence. No build or dependency provisioning
occurs implicitly. Other platforms report `not_run` with exit 3.

Each run creates a fresh ignored `artifacts/cli-trace/<uuid>/` directory. Each
case has separate workspace, configuration home and temporary directory. The
child gets an allowlisted environment, redirected home/config paths and a
synthetic local token. It ignores user config/rules, disables project instruction
loading, telemetry/feedback settings, updates, web search, memory and shell
snapshots, and uses ephemeral sessions. These are experiment settings, not
implemented VCP security controls or proof that all other traffic is impossible.

## Cases and independent checks

| Case | Scripted response | Independent acceptance |
|---|---|---|
| `completion` | One complete assistant message | Exactly one request, expected model/prompt/auth and one completed turn/message |
| `patch-read-only` | Fixed `apply_patch` adding `synthetic.txt`, then final message | Two requests; rejection receipt reaches the second request; file is absent and no successful file-change event appears |
| `patch` | Same patch and final message with explicit unsandboxed fixture execution | Two requests; success receipt, successful file-change event and exact independently read file bytes |
| `retry` | HTTP 503 followed by completion | Exactly two requests preserving input, one successful completed turn |
| `provider-denied` | HTTP 401 | Exactly one request, nonzero child exit, failed turn and no completed turn |

Only the fixed synthetic patch case uses `--sandbox danger-full-access`. It
contains no shell command or freely generated tool request. All other cases
explicitly request read-only execution and every case disables approval prompts.
The ordinary `workspace-write` probe was rejected on this uninitialized Windows
sandbox installation; the failure remains recorded. Successful unsandboxed
patching cannot qualify filesystem/process enforcement in P0-05.

Requests are bounded to 4 MiB and process output to 8 MiB. Unexpected routes or
extra requests fail. The 60-second process deadline attempts to terminate only
the live owned child tree; unconfirmed cleanup fails explicitly. Timeout or
process termination is not VCP pause. Raw request bodies, stdout/stderr and
process receipts remain in the ignored case directory, including on failure.
Evidence directories are preserved, not broadly cleaned.

## Implementation and limits

The reusable fixture and independent oracle are in
[`cli-trace.cjs`](../../src/tests/support/cli-trace.cjs); orchestration is in
[`trace-cli.cjs`](../../scripts/upstream/trace-cli.cjs). CI runs their synthetic
HTTP/oracle regression tests via the `fast`/`upstream` suites on `ubuntu-8core`.
Those tests reject false completion, hidden retries, incorrect authentication,
missing/incorrect patch contents and receipt-only success. Linux regression
success is separate from the native binary experiment.

The wire shapes and CLI settings were checked against the pinned Codex
`core/tests/common/responses.rs`, `core/tests/suite/cli_stream.rs`,
`exec/src/cli.rs` and `model-provider-info/src/lib.rs`. No upstream source was
modified or additional third-party code imported. The fixture uses synthetic
messages and protocol field names rather than real transcripts.

Continue P0-03 at the [controller source map](codex-boundaries.md), preserving
the [engine lifecycle contract](../architecture/engine-execution-design.md).
These five traces cover neither root/child in-app pause nor review, compaction,
memory, realtime or all credential/network helpers. They use upstream usage
events, not VCP's atomic budget ledger. [Executed evidence](../evaluations/p0-07-cli-trace.md)
records the precise qualified scope.
