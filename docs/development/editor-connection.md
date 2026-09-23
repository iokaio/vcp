# Initial VS Code connection

Work item: P4-01, first connection increment. P8 is closed by
[owner direction](../adr/042-owner-directed-p8-closure.md), and P9-01 through P9-03
are complete. This increment begins the editor integration described in
[ADR-056](../adr/056-editor-observer-connection.md). It does not accept all P4-01
requirements.

## Connection boundary

The extension connects the prepared workspace/connection presentation to the
TypeScript SDK and authenticated local engine. Connection is explicit, scoped to
one selected existing workspace and read-only. Configuration comes from the user,
never a project-suggested executable. The extension exposes actual engine state
and closes stale connections when selected roots change.

Engine absence, failed negotiation, unavailable store and mismatched workspace
state leave the extension disconnected with an actionable bounded diagnostic.
Refreshing a view cannot acquire control, answer a decision or resume a task.
Only the extension's own SDK connection is disposed.

Set `vcp.engineExecutable` in VS Code **User settings** to the trusted absolute
`vcp.exe` path. Set `vcp.dataDirectory` there if the initialized workspace uses a
different data directory from the engine default. Open the VCP activity-bar view,
choose **Connect**, and select the folder by its full URI. **Refresh** reads current
state and **Disconnect** closes the connection. Equal display names remain distinct.
Editor trust and canonical engine trust are shown separately; restricted-mode
observation cannot change either engine policy or task execution.

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

## Remaining P4-01 acceptance

The public API needs the actual root/binding projection, durable trust mutation
and moved-root reconciliation, plus authenticated observer handoff across host
reload. The current UI does not fabricate these capabilities. Complete P4-01 also
requires actual queued-tool trust revocation, moved-root identity preservation,
reload during a pending question and observing reload while another controller
survives. These requirements remain in the
[owning contract](../plan/18-deferred-vscode.md#p4-01--connection-and-workspace-mapping).

Full task/child views and approval actions belong to P4-02, prepared document
changes to P4-03, inspectors to P4-04 and clean installation/update compatibility
to P4-05. No VSIX or extension-marketplace release is published here.
