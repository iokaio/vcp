# VS Code workspace connection

Work item: P4-01. P8 is closed by
[owner direction](../adr/042-owner-directed-p8-closure.md), and P9-01 through P9-03
are complete. [ADR-056](../adr/056-editor-observer-connection.md) records the initial
observer increment; [ADR-057](../adr/057-editor-trust-and-observer-recovery.md)
records the subsequent trust and recovery boundary.

## Connection boundary

The extension connects the prepared workspace/connection presentation to the
TypeScript SDK and authenticated local engine. Initial connection is explicit and
scoped to one selected existing workspace. Observation is the default; controller
acquisition is explicit. Configuration comes from User settings,
never a project-suggested executable. The extension exposes actual engine state
and closes stale connections when selected roots change.

Engine absence, failed negotiation, unavailable store and mismatched workspace
state leave the extension disconnected with an actionable bounded diagnostic.
Refreshing a view cannot acquire control, answer a decision or resume a task.
Only the extension's own SDK connection is disposed. A saved observer reference
can authenticate to the same live engine after reload, without starting a writer,
acquiring control or replaying commands. It contains no ticket or credentials.

Set `vcp.engineExecutable` in VS Code **User settings** to the trusted absolute
`vcp.exe` path. Set `vcp.dataDirectory` there if the initialized workspace uses a
different data directory from the engine default. Open the VCP activity-bar view,
choose **Connect**, and select the folder by its full URI. **Refresh** reads current
state and **Disconnect** closes the connection. Equal display names remain distinct.
Editor trust and canonical engine trust are shown separately. **Connect as
controller** explicitly acquires an available controller lease. **Trust engine
workspace** requires editor trust; **Revoke engine trust** remains available in
restricted mode to the controller. Both operations pause and drain owned work and
return the editor to observation. Granting editor trust alone never grants engine
trust. Losing the editor controller during a trust-revocation reload fences its
queued work. An observing editor cannot alter another controller's trust or lease.

**Reconcile moved root** selects the destination by URI and asks for the existing
workspace ID. Close its engine owner first; disconnected pipe engines retain the
writer for 30 seconds. Native reconciliation preserves identity/history, advances
binding revision, resets trust and leaves tasks paused. An active owner causes
refusal. The extension never reads private workspace descriptors.

**Observe existing engine** accepts a non-secret reference from
`Client.observerReconnectReference()`. Reload persists that reference with the
selected URI and User-profile digest, then re-authenticates native process and
executable identity and queries fresh pending state. It does not persist the
process-local `LocalAttachment` or controller credentials. Profile changes or a
dead/replaced server require explicit connection; no fallback launches a writer.
Explicit Disconnect clears recovery. The native 30-second idle interval also
limits reload recovery when no other client keeps the server alive.

This extension/SDK requires the native observer-reconnect bootstrap opt-in. Older
engines fail closed; older SDKs keep their original readiness response when using
the new engine. P4-05 owns packaged compatibility and upgrade qualification.

The root/binding projection increment requires the negotiated
`workspace/binding/1` capability. The map and Workspace view use the engine's
opaque `root_id` and decimal-string `binding_revision`, alongside workspace and
execution-host IDs. Binding revisions are independent of workspace and authority
revisions and retain full integer precision. The current engine primary root uses
the workspace's opaque identity; the extension consumes the returned value rather
than deriving it. An older engine or an incomplete projection leaves the connection
unavailable. Folder changes still invalidate the complete map.

## Development build

The private extension package is `src/packages/vscode`. Its locked development
inputs are TypeScript 5.9.3, Node type definitions 24.10.1 and VS Code type
definitions 1.138.0. The selected editor engine selector is `1.138.0`, with actual
runtime qualification against 1.138.0. Runtime code uses the existing SDK and
generated protocol package, without another provider or transport implementation.

From the repository root:

```powershell
npm ci --prefix src/packages/sdk-ts --ignore-scripts --no-audit --no-fund
npm ci --prefix src/packages/vscode --ignore-scripts --no-audit --no-fund
npm test --prefix src/packages/vscode
npm run stage --prefix src/packages/vscode
```

The final command builds a self-contained development extension directory at
`artifacts/p4-vscode-extension`. Staging copies the SDK distribution and canonical
schema and removes development links from the manifests. It does not publish a
VSIX, install into the user's profile or bundle a native engine.

## Qualification

The clean offline install and strict extension/SDK build passed all 16 portable
tests. They cover actual scoped read requests, bounded pagination and unsubscribe,
stale launch/disposal, URI/host mapping, independent trust states, missing or
incompatible peers, User-only configuration, cancelled picker intent, closed
webview messages, text-only rendering and standalone package imports. Staging
tests also preserve unknown directories and reject redirected destinations.
The build began without dependency, compiled output or staging directories. Evidence is in
`artifacts/p4-clean-editor-tests-final.log` and the two
`p4-clean-*-install-final.log` files.

The repository fast gate passed all 18 cases, manifest
`d2bd206d-0e07-42eb-a0d0-6bc470989102`, with output in
`artifacts/p4-final-fast.log`. CI also builds/tests the extension on its routine
Linux job; the native Windows job remains separately selected.

The actual editor-host gate is `vcp-cli --test local_editor` with feature
`qualification`. It is ignored by default because it requires an explicitly
selected editor (`VCP_TEST_CODE`) and the staged package. Run it with
`--ignored`; an ordinary CLI test run is not editor-host evidence. It uses a
private profile with workspace trust enabled and a compiled native engine,
without a configured provider. Node-only tests remain distinct from this gate.

On September 23, 2026, the native gate passed one test covering **both Files and
SQLite stores** in 4.39 seconds. The actual extension host reported VS Code
1.138.0 and Node 24.18.1, with workspace trust enabled and `isTrusted=false`.
Registered Connect, Refresh and Disconnect commands inspected the correct
canonical workspace/host/root and retained pending decision in two folders with
the same display name. The Workspace view provider initialized, and SDK/schema
resolution stayed inside the staged extension. Missing User executable settings,
project overrides, a nonexistent configured executable and an unselected folder
could not establish a connection. Independent full canonical-state equality
before and after each store proved no acquired lease, resumed work, answered
decision or changed receipt/effect. Renderer text safety is separately covered by
the portable tests; view initialization is not a screenshot or DOM assertion.

The machine's installed editor was waiting on its updater lock. Qualification
used Microsoft's independent **1.138.0 Windows x64 ZIP**, following the official
[versioned download route](https://code.visualstudio.com/docs/supporting/faq#_previous-release-versions)
and [ZIP distribution guidance](https://code.visualstudio.com/docs/setup/portable).
The archive SHA-256 is
`c0a9f12a0d8962fa4cda1959eecd5fdd3f82de9715ad4dadd5601856bb60fd21`;
its Microsoft-signed executable had valid Authenticode status and SHA-256
`081417b5e8032c703cf9e25e4886ab10c417ad1b8b87cfd1244c67d15331a8a6`.
The installation, updater, editor product files, trust policy and sandbox flags
were not modified. The first archive run exposed the unsupported `~1.138.0`
manifest selector; the corrected exact selector passed real extension registration.

Evidence is `artifacts/p4-editor-archive-final-tests.log`,
`artifacts/p4-extension-host/result.json` and
`artifacts/p4-editor-archive-1.138.0/provenance.json`. Earlier process-token,
updater and manifest failures remain in the corresponding diagnostic logs; they
are not passing qualification. Reproduce the native case with a configured
Windows Rust/MSVC environment after staging:

```powershell
$env:VCP_TEST_CODE = 'C:\path\to\extracted\Code.exe'
cargo +1.95.0 test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline -p vcp-cli --features qualification --test local_editor -- --ignored --nocapture
```

## Root/binding projection qualification

The projection increment passed 16 extension tests, 30 SDK tests (including the
compiled examples), 32 protocol tests and three engine capability tests. The
engine tests exercise initialized RPC serialization, strict legacy decoding,
missing required capabilities and counters above JavaScript's exact integer
range. The repository fast gate passed all 18 cases, manifest
`d30a7cf3-2487-46e0-ad32-ffb6d17dc81a`.

Both `public_workspace` lifecycle tests passed with the `qualification` feature,
covering Files and SQLite. They verify exact counters, unchanged observer state,
access rechecks and a stable root identity after reopening a durable rebind with
its incremented binding revision. This is projection evidence; editor-driven
moved-root reconciliation remains below.

The native `local_editor` gate passed against VS Code 1.138.0 and Rust 1.95.0,
checking actual `rootId` and `bindingRevision` for both stores in restricted mode.
Canonical-state equality still proves that observation did not acquire a lease,
answer the retained decision or resume execution. Evidence is
`artifacts/p4-binding-editor.log` and `artifacts/p4-extension-host/result.json`.

## P4-01 acceptance

P4-01 is accepted on the qualified local Windows host. The final native editor
gate passed against VS Code 1.138.0 with Node 24.18.1 and Rust 1.95.0. It used one
editor launch with normal persistent workspace storage and observed a different
extension-host process after `workbench.action.reloadWindow`. The isolated
development driver replaces `--extensionTestsPath`, which selects in-memory
storage and cannot qualify durable recovery. Editor trust and sandbox settings
were not weakened.

The gate preserves exact canonical state for two identically named roots using
Files and SQLite. A physically moved root retains workspace/root identity,
increments its binding revision, resets trust, and keeps its task paused and
question pending. In restricted editor trust, granting engine trust is rejected;
explicit revocation increments canonical workspace revision and authority and
returns the editor to observation. A real observer reload automatically restores
the pending question while an external client's controller lease remains unchanged.

Final verification passed 26 extension tests, 33 SDK tests and TypeScript builds,
32 protocol tests, five native pipe tests, the scope-identity unit test and two
trust lifecycle tests, including queued-startup revocation and drain. The compiled
SDK pending-question test passed across both stores with a fresh observer process;
it compares controller authority exactly and checks the independent global store
watermark for monotonicity. The repository fast gate passed all 18 cases (manifest
`5a896785-e0a8-408b-9071-dcb2076f4fe3`).

Native evidence is `artifacts/p4-editor-final.log`,
`artifacts/p4-extension-host/result.json`, `artifacts/p4-sdk-pending-final.log`,
`artifacts/p4-pipe-final.log`, `artifacts/p4-protocol-final.log`,
`artifacts/p4-trust-scope-final.log` and `artifacts/p4-trust-final.log`.
The [owning contract](../plan/18-deferred-vscode.md#p4-01--connection-and-workspace-mapping)
and [ADR-057](../adr/057-editor-trust-and-observer-recovery.md) define the scope:
local Windows only, explicit controller acquisition, no automatic mutation replay,
and observer recovery subject to the server's existing 30-second idle retention.
Switching a self-launched observer-only engine to an explicit controller connection
can therefore require waiting for that idle shutdown and retrying. The observer
reference cannot upgrade its role; another live controller must release ownership
before a replacement engine can open the workspace.

Full task/child views and approval actions belong to P4-02, prepared document
changes to P4-03, inspectors to P4-04 and clean installation/update compatibility
to P4-05. No VSIX or extension-marketplace release is published here.

The subsequent [task and child view](editor-tasks.md) has its own P4-02
implementation and qualification evidence.
