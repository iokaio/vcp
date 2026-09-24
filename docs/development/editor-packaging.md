# Editor packaging and compatibility

Work item: [P4-05](../plan/18-deferred-vscode.md#p4-05--packaging-and-compatibility).
Status: P4-05 accepted on September 24, 2026, for the Windows/VS Code envelope
below. Actual VSIX installation and upgrade qualification supplements the accepted
P4-01 through P4-04 editor behavior.

## Distribution decision

The extension is a Windows VSIX with its exact SDK and generated protocol schema
bundled. The native engine remains a separately installed Windows package. Configure
an absolute trusted engine path in VS Code **User settings**. Workspace settings,
remote URLs and project content cannot select or install an executable. There is no
managed downloader or automatic engine replacement.

Use the official pinned VS Code Extension Manager to construct the VSIX from the
allowlisted stage. The archive contains extension code, media, required SDK/schema
files, package metadata and applicable notices. Test drivers, runtime history,
provider credentials and recovery material do not belong in it. See Microsoft's
[packaging instructions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension).

Version labels alone do not prove compatibility. The qualified pair must identify
the VSIX digest, native executable/package digest, source revision, SDK/schema
versions and actual negotiated capabilities. Protocol, event schema and schema
version are each `1.0`; an incompatible handshake is unavailable. Individual
features also require their negotiated method and profile. The engine build label
`0.1.0` is not a promise that every historical binary supports editor features.

Build the native candidate with `scripts/package.ps1`, then run `npm ci` and
`npm run build` in `src/packages/vscode`. From that directory, construct the VSIX:

```powershell
node scripts/package.cjs --engine D:/trusted-candidate/package/vcp.exe --engine-manifest D:/trusted-candidate/result.json
```

The output is `artifacts/p4-vscode-package`: the VSIX, its complete hashed inventory
and the compatibility manifest. Official VSCE 4.0.0 is a pinned development tool;
neither VSCE nor the ZIP inventory reader is shipped as an extension dependency.
`--version 0.1.1` creates a separately labeled qualification successor from the
same source solely to exercise VS Code's version-update path. It does not change
the SDK or establish compatibility with a different engine implementation.

## Update and recovery sequence

1. Preserve the existing canonical data directory and independently held recovery
   keys. Record the current package hashes and configured engine path.
2. Close active engine owners before replacing an installation. Install the new
   native package in a distinct directory and verify its inventory. Preserve the
   prior installation until the new pair has been checked.
3. Install the matching VSIX and select the intended native executable in User
   settings. Reconnect to the existing initialized workspace explicitly when its
   executable profile changes.
4. Check the reported workspace/root identity, binding revision, engine build and
   pending command outcomes. Reload restores observation; acquiring control and
   resuming work remain explicit actions. Never replay an uncertain mutation to
   test an upgrade.

A self-launched observer engine retains its writer for up to 30 seconds after
disconnect. Wait for that owner to exit before launching the replacement against
the same canonical store. An independently controlled engine belongs to its
controller; disconnecting the editor does not authorize terminating it.

A failed install or handshake does not authorize deleting data, clearing receipts
or selecting another engine automatically. Preserve the failed artifact and use
the recorded compatible pair where store migration compatibility permits it.
Restoring an old binary does not undo a canonical migration. Independently stored
recovery material is not an extension setting and must survive uninstall.

`scripts/qualify-native-upgrade.ps1` in the extension package's source directory
accepts two distinct native package archives and writes bounded JSON evidence.
Its native-installer fixture passed interruption after staging/before pointer
replacement, retry, incompatible-state rejection and actual installation removal.
Independent data and key-material sentinels remained byte-identical. This is a
deterministic native activation fault, separate from VSIX truncated-download
rejection and from canonical-state/editor-buffer qualification below.

## Qualification record

| Component | Qualification envelope |
| --- | --- |
| Operating system | Windows x64, build 10.0.26200.0 |
| Editor and extension API types | VS Code 1.138.0 |
| Extension | `vcp.vcp-local` 0.1.0; 0.1.1 is a same-source update fixture only |
| SDK and protocol package | 0.1.0, bundled; schema digest recorded per artifact |
| Negotiated wire versions | Protocol, event schema and schema each 1.0 |
| Build tools | Node 24.21.0, TypeScript 5.9.3, VSCE 4.0.0; locked dependencies |
| Native candidate | Unsigned local package, executable digest bound by VSIX manifest |

The runtime does not require a separately installed Node or npm. The editor
supplies its extension host; the native engine and its declared assets are
installed separately. Capability negotiation remains authoritative for each
feature even when the displayed version labels match this table.

The final candidate identities are:

| Artifact | SHA-256 |
| --- | --- |
| Native executable | `7ecda7471ff54c826c419977e4357564c139e3e96a24a34d7fe9f5f6dbc2d22d` |
| Native ZIP | `144ae7aa4337fcfa27bf11b10f5f57d2685c67da64de4004b8934d8a7b8ebed3` |
| Extension 0.1.0 VSIX | `e243a0d12b114bff19de1ac8392444211a4d3b784d3b74e5706137874802d6cd` |
| Qualification successor 0.1.1 VSIX | `5d7b949659c4944dec7481f6486dbcd6836b270b1cf92a6c208d33a2273d28f9` |

Both VSIX inventories contain 58 entries, including the license, notice and
referenced third-party attribution file beside each shipped package. They contain
no native engine, test driver, stale build module or source map. Source metadata
records a dirty working tree and the native build as caller-supplied/unverified;
the byte-bound tests below are separate from formal release-build provenance.

The acceptance gate uses actual VSIX installation in private native editor profiles,
with the package and engine installation outside the development checkout and no
development PATH. The final lifecycle gate passed on Files and SQLite in 317.91
seconds: clean activation, actual reload and restart, rejected truncated VSIX with
a usable original installation, successor installation, engine replacement at a
distinct path, missing-engine diagnostics and uninstall. Canonical paused state
and independent key-material bytes were preserved. Replacement used the same
qualified native build; actual unsupported protocol negotiation does not establish
arbitrary historical-binary downgrade compatibility.

The actual VSIX buffer-workflow gate passed on Files and SQLite in 124.18 seconds
using the final candidate above. It retained the existing P4-03 assertions for
different roots, close/reopen, rename/delete, actual typing before stale apply,
partial file edits, save/close observations, an interrupted final receipt, undo
before reconciliation and dirty-buffer reload without duplicate application.
Source text and version advancement were observed before testing stale review;
merely submitting the asynchronous editor typing command is not that precondition.
The product and qualification driver were separately installed through the actual
VS Code VSIX installer in private profiles, with runtime code outside the checkout.

The 130-version history startup probe reproduced the readiness failure on both
stores. Store reopen took 24.515 seconds on Files and 25.155 seconds on SQLite;
canonical-host open took 24.891 and 25.474 seconds respectively. Each unchanged
native bridge failed before readiness; the SDK reported its bootstrap timeout.
These are fresh process/store opens with a warm operating-system cache, not a
claim of cold-disk performance. Separate bounded launch readiness and immediate
owned-child cleanup passed the qualification below. Authentication, readiness and subsequent initialization are
separate deadlines; increasing only the SDK deadline cannot fix the native bound.

With separate bounded launch readiness, the 130-version qualification passed on
both stores in 265.92 seconds. Files native readiness took 27.205 seconds and its
SDK launch/initialize took 26.169 seconds; SQLite took 24.760 and 24.495 seconds.
Initialization itself took 11–12 milliseconds. EOF and an unexpected pre-ready
frame stopped only the newly owned server in 12–16 milliseconds, verified through
independent process liveness. Canonical state remained equal. A subsequent narrow
fix makes other startup failures use the same immediate cleanup instead of the
post-connection graceful drain; its real-child regression passed (1 test). The
final native build also passed attachment (5 tests), authenticated pipe/reconnect
(5 tests) and ordinary inspector queries on both stores (1 test). The 130-version
probe preceded that error-only cleanup refinement and was not rerun against the
final artifact hash.

Portable validation passed: 158 extension tests, 35 SDK tests and all 18 fast
repository checks. Final native logs are `artifacts/p4-package-editor-native.log`
and `artifacts/p4-package-workflow-native.log`; native installer fault/recovery
evidence is `artifacts/p4-native-upgrade-result.json`. Package inventories bind
the final artifact identities above. Rust formatting, script syntax, repository
contracts and diff whitespace checks passed.

Only Windows and VS Code 1.138.0 have this package/editor evidence. Other VS Code versions,
operating systems, WSL, SSH, containers and remote/virtual workspaces are unqualified.
Unsigned local package qualification does not imply marketplace publication, a
signed release or fresh-machine support.
