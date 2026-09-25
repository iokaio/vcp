# CS-2 Windows Node execution prerequisite

This qualification-only adapter executes one JSON computation in a fresh owned
x64 Windows AppContainer. It prepares a CS-2 execution prerequisite; it does not
complete CS-2, qualify a skill, or change the product sandbox. No model calls,
downloads, provider credentials, browser or external endpoint are involved.

## Boundary

`scripts/evals/node-fixture-runner.ps1` accepts a bounded configuration containing
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

Earlier failed startup, console-member and TCP-control diagnostics remain in the
ignored local artifacts. The TCP control initially waited for a server close
before the synchronous supervisor could accept; correcting the control handshake
made both independent nonce observations succeed. Node child-process creation
may stall under the one-process boundary; that is a bounded timeout failure, not
a claim that every attempted spawn returns a particular Node error.

## Remaining acceptance

This adapter establishes only externally observable JSON behavior. It does not
authenticate child-reported Map snapshots, iterator read counts, injected
transport observations, mutation reports or exception identities. Existing
CS-2 semantic assertions that depend on those same-process observations still
need a qualified interaction design; their output cannot be promoted to trusted
parent evidence by wrapping it in JSON. No held-out generated artifact has been
run by this prerequisite.

Real MCP stdio/protocol and selected SDK compatibility, live provider evidence,
browser/DOM behavior, human usefulness and the CS-0 comparison gates remain
unqualified. This is a single-process Windows fixture boundary, not arbitrary
tool-tree support, another host qualification or a production sandbox claim.
