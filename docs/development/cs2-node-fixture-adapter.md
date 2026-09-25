# CS-2 Windows Node execution prerequisite

This qualification-only adapter executes one JSON computation in a fresh owned
x64 Windows AppContainer. It prepares a CS-2 execution prerequisite; it does not
complete CS-2, qualify a skill, or change the product sandbox. No model calls,
downloads, provider credentials, browser or external endpoint are involved.

## Boundary

`scripts/evals/node-fixture-runner.ps1` accepts a bounded single-shot configuration containing
the installed Node path/hash, bootstrap hash, flat candidate inventory/hashes,
one base64-encoded input and explicit resource ceilings. The inventory must
contain `candidate.cjs`; its exported `compute(input)` returns a JSON value.
The trusted parent supplies a random request ID and checks the resulting value
with `node-fixture-protocol.cjs`. The child cannot certify its own correctness.

The runner reads and hashes bounded source snapshots, copies only declared files,
creates its own CommonJS package boundary and replaces the owned profile ACL with
read/execute access for the container. Candidate execution has no writable fixture
files. Staged fixture bytes are checked against the captured inputs after
execution. Configuration, inventory and input ceilings are respectively 256 KiB,
1 MiB and 64 KiB; at most
sixteen flat `.cjs`/`.json` files are allowed. Bootstrap and package names are
reserved. Reparse source paths, duplicate names and hash drift fail closed.

The new `AppContainerFixture.RunBounded` API preserves the original `Run` callers.
It creates a zero-capability token, verifies its exact profile SID, assigns the
suspended process to a kill-on-close Job Object and only then resumes it. Only
the three selected pipe handles are inherited. The environment contains eight
explicit Windows/profile variables; provider, proxy and Node option variables
are not inherited. `--no-addons` disables native addons. Node receives literal
loader paths and an owned package boundary so that startup does not need to walk
inaccessible parent directories.

The bounded API uses `DETACHED_PROCESS`: initial live tests with
`CREATE_NO_WINDOW` observed an extra `conhost` member. The new API observes job
membership and rejects more than one active process. It enforces process/job
committed-memory ceilings from 64 MiB to 1 GiB, a wall deadline up to 180 seconds,
at most 64 KiB input and at most 1 MiB combined retained stdout/stderr. Output
overflow stops the job while drains remain bounded. Cancellation and failure
paths close the owned job and independently observe process exit. Normal cleanup
deletes the unique owned profile. After abrupt owner death the supervisor must
reconcile that recorded profile; the owner-loss qualification exercises this
explicitly.

## Interactive mode

A request with `"mode": "interactive"` replaces `input_base64` with an
`interaction` object: `max_frames` (at most 4096 per direction), `max_frame_bytes`
(at most 64 KiB), `max_total_bytes` (at most 1 MiB per direction) and `idle_ms`
(at most `timeout_ms`). Its `output_limit` bounds retained stderr and may not
exceed 64 KiB. It stages the separately hashed
`node-fixture-interactive-bootstrap.cjs` as `bootstrap.cjs`, so single-shot
requests, their bootstrap and their receipts are unchanged.

`AppContainerFixture.RunInteractive` uses the same token, job, environment,
memory and wall-deadline path as `RunBounded`. It relays newline frames between
the runner's own stdin/stdout and the contained child instead of writing one
fixed input. Output to the parent is one JSON envelope per line: `started`
(child PID and owned profile name, for supervisor reconciliation), `frame`
(the child's line as opaque base64) and a final `receipt`. The relay never
interprets frame content.

Any of the following terminates the job, and each records its own termination:

| Cause | Termination |
|---|---|
| Too many frames | `frame_limit` |
| A frame over the size ceiling | `frame_bytes` |
| Too many bytes in total | `total_bytes` |
| Stderr over its ceiling | `output_limit` |
| No frame either way for `idle_ms` | `idle_timeout` |
| The wall deadline | `timeout` |
| Cancellation | `cancelled` |

When the parent causes a violation, the termination is prefixed `parent_`, for
example `parent_frame_limit`. An unterminated final parent line is
`parent_partial_frame`. The parent's session header counts toward
`max_frames`.

Frame and byte counts are observed values. They include the frame that crossed
a ceiling, so a `frame_limit` run reports one more frame than was relayed.
Bytes after the child's last newline are counted as `trailing_bytes`; this is
not recorded after a ceiling violation.

Parent end-of-input closes the child's input. The bootstrap keeps reading
until then, so the parent must close within `idle_ms` of its last exchange.
The idle clock starts before Node boots, so very small `idle_ms` values are
fragile.

`started` is emitted when the relay starts, before job assignment and token
verification complete. It identifies the profile for reconciliation and is not
proof of containment; the receipt is.

The interactive bootstrap requires a first `{"session"}` frame and gives the
candidate a channel whose outgoing frames carry `{session, seq, body}`.
[`node-fixture-session.cjs`](../../scripts/evals/node-fixture-session.cjs) is the
trusted parent helper.

- `decodeFrame` decodes each frame. It requires canonical base64, strict UTF-8
  JSON, the exact key set, the session and the next sequence number. A rejected
  frame fails the whole session.
- `checkInteractiveReceipt` accepts only a clean exited AppContainer process
  with no stderr or trailing bytes, no unread child frames, and frame counts
  equal to the parent's own counts.
- `close()` waits for the configured `timeout_ms` plus a cleanup margin.
- A run that ends without a receipt never passes. This covers a missed close
  deadline, a helper-detected envelope violation and abrupt owner loss. The
  helper, or the supervisor after owner loss, waits for the recorded child PID
  to exit and then deletes the recorded profile with `reconcileProfile`.

The session and sequence are not secrets from the contained process. They catch
confused, duplicated, reordered and unrequested frames, not a candidate that
lies about its own behavior.

What this mode adds is that the trusted parent can play a transport, iterator or
protocol peer. An outgoing request, retry or iterator pull that the protocol
turns into a frame is then observed outside the container, not reported from
inside it. Reads with no outgoing frame remain unobserved. A probe still judges
only the frames it receives.

## Reproduction and evidence

Run the portable protocol tests and the actual Windows integration tests:

```powershell
node --test src/tests/contracts/node-fixture-protocol.test.cjs src/tests/contracts/node-fixture-windows.test.cjs
pwsh -NoProfile -File scripts/evals/node-fixture-qualification.ps1 -Node 'C:/Program Files/nodejs/node.exe' -NodeSha256 ba4e6d110e8c1592a1ecd390f6b05f3da124b13871a5be62b341a07a853c6c32 -Output artifacts/node-qualification.json
```

The native qualification requires that exact installed executable; it never
installs or substitutes Node. The separately run Windows integration test pins
its executing Node binary for that invocation. Portable protocol assertions are
registered in the fast harness; native AppContainer evidence remains an explicit
Windows qualification command.

The September 25, 2026 local run used Windows 10.0.26200.0 x64 and Node 24.21.0
with the hash above. Four test
cases passed, including thirteen actual runner invocations covering good output,
read-only fixture preservation, wrong results, forged verdicts, extra frames,
stderr, early exit and prelaunch hash/path/input/memory rejection. Eleven native
cases passed. Native checks
cover the original `Run` API, environment filtering, attempted process creation,
combined output flood, memory exhaustion, timeout, cancellation, outside
read/write and replaced-junction denial, TCP denial with two successful
unrestricted controls, and abrupt owner loss with supervisor cleanup.

A later September 25, 2026 run on the same host, Node binary and hash
added interactive mode. Seven JavaScript tests passed, including the unchanged
single-shot integration test. The interactive integration test drives the real
runner through `node-fixture-session.cjs`. It covers:

- a parent-owned transport whose request and single call are observed outside
  the container;
- overlapping candidate reads;
- a forged session, and a duplicate and a skipped sequence number emitted with
  the correct session by stdout interception; each is rejected by its specific
  assertion, not by a timeout;
- non-JSON output and unrequested frames;
- frame-count, frame-size, total-byte, parent frame-count and idle ceilings;
- explicitly observed trailing bytes and stderr, and early exit;
- a missed parent close deadline whose profile is reconciled rather than leaked;
- seven prelaunch rejections: unknown mode, `input_base64`, missing or
  excessive bounds, oversized `output_limit`, and the single-shot bootstrap hash;
- abrupt owner loss mid-exchange. The child PID exits, the recorded profile
  demonstrably remains, and the supervisor then deletes it.

Fourteen native cases passed: the eleven above plus an interactive relay, a
frame-count ceiling and an idle deadline driven directly through
`RunInteractive`.

Earlier failed startup, console-member and TCP-control diagnostics remain in the
ignored local artifacts. The TCP control initially waited for a server close
before the synchronous supervisor could accept; correcting the control handshake
made both independent nonce observations succeed. Node child-process creation
may stall under the one-process boundary; that is a bounded timeout failure, not
a claim that every attempted spawn returns a particular Node error.

## Remaining acceptance

Both modes establish only externally observable behavior. Neither authenticates
child-reported Map snapshots, mutation reports or exception identities. Their
output cannot be promoted to trusted parent evidence by wrapping it in JSON.

Interactive mode is the qualified interaction design for transport calls,
iterator reads and protocol exchanges. It works only when the parent owns that
peer and makes each observation itself. Existing CS-2 semantic assertions must
be rewritten as parent-side probes over these two modes before they count as
evidence. No held-out generated artifact has been run by this prerequisite.

Real MCP stdio/protocol and selected SDK compatibility, live provider evidence,
browser/DOM behavior, human usefulness and the CS-0 comparison gates remain
unqualified. This is a single-process Windows fixture boundary, not arbitrary
tool-tree support, another host qualification or a production sandbox claim.
