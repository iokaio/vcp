# CS-3 WebView2 diagnostic source

Current-host continuation variant (not yet qualified): based on the retained
isolated-worker source merged in PR #198, now pins this host's observed WebView2
`154.0.4258.37` executable SHA256
`3f48b1ab9a5d5e65a96307b6655e29882bd70bb682ce4b67a9e7a7f07f61019d`.
The unavailable worker's receipts remain historical evidence in the native DOM
checkpoint. This fresh diagnostic uses the same explicitly versioned
Microsoft.Web.WebView2 NuGet package `1.0.4191.47`,
package SHA256 `f492bbf547d0da329553b6727435b677579b1e9f91cc9e4a1ad029366d5f23d0`.
Core (`lib/net462`) SHA256 is
`e6f54c8ce208e3797c427d01ad671b47cb25abc85604753d6ec2546d0ffef550`;
loader (`build/native/x64`) SHA256 is
`c66e4a92fdc7a216118e43b7a5024ea2200e8c43f9310bf20d96a0084f82c5bc`.
Both vendor LICENSE and NOTICE accompany staged inputs. This is a fresh source
variant, not a reproduction or pass of the earlier diagnostic. The original
SDK pins below are historical and remain in Git history; compatibility and a
new browser-free build must be checked before reviewing any native launch.

The first isolated-guest attempt stopped during scratch observation after the
WebView2 environment was created. A new source-bound diagnostic records one
bounded, root-relative `scratch_open_failure` event when `CreateFile` fails,
including the original native code and unchanged access/share/flag values. It
never records the absolute profile root. Missing child errors 2/3 remain the
only skippable hint/open race; metadata, reparse, sharing and access failures
remain fatal. This is diagnostic context, not a retry, ACL change, relaxed
observer or browser-qualification result.

This directory contains original VCP diagnostic source imported from the retained
local GUI draft. It is test support, not a browser product component or imported
third-party sample. The build keeps these critical sources here, snapshots their
hashes, and stages copies into a new repository `artifacts` directory. Generated
executables, copied pinned dependencies, compiler temporary files, test binaries,
and receipts belong only in that new build directory; do not build in this source
directory.

The browser-free native guardian smoke is a separate explicit command:

```powershell
pwsh -NoProfile -File src/tests/support/windows/webapp/Test-WorkerGuardian.ps1
```

It launches only fixed waiting PowerShell helpers, verifies their held process
identities, and tests both explicit guardian disposal and abrupt sole-owner loss.
It creates no browser profile and changes no account, policy or ACL. Both helpers
have bounded fallback lifetimes; the parent also cleans its exact held helpers.

The diagnostic pins the original external WebView2 inputs:

- `Microsoft.Web.WebView2.Core.dll` SHA-256
  `88a3b62f45225a811cdb85df6dfd95c2bff9a0e43e3b04f813b125eaca56cc9f`
- x64 `WebView2Loader.dll` SHA-256
  `462b36fd1be6ca9f7563466a89e57c41ef4a4def3e0a84fa885d203aea4a3aaf`

Build from native x64 Windows with PowerShell 7, the .NET Framework 4.8 reference
assemblies, and the framework x64 C# compiler already installed:

```powershell
pwsh -NoProfile -File scripts/evals/webapp-browser-build.ps1 `
  -CoreAssembly C:\absolute\pinned\Microsoft.Web.WebView2.Core.dll `
  -Loader C:\absolute\pinned\x64\WebView2Loader.dll `
  -OutputDirectory D:\code\Github\vcp\artifacts\cs3-webview2-NEW
```

The output directory must not exist. The builder stages and hashes all inputs,
records the compiler and reference assemblies, compiles the host and pure tests,
and runs only compile/pure contract checks. It does not launch WebView2 or another
browser. Inspect the emitted `inputs.json`, source/toolchain hashes, build output,
and the complete native execution plan before any launch.

The fixed form records passive script readiness and separate trusted readiness,
ordinary-key and submit counters; read-only snapshots are checked independently
by the parent. Native forward focus is not a claim of CDP Tab input. The retained
[routing report](../../../../../docs/development/cs3-input-routing-diagnostic.md)
records the successful and unsuccessful timing controls and remains unchanged.

The current source-bound build is a fresh full-DOM diagnostic. After each fixed
100-ms pre-dispatch settle window it sends one exact left-Shift down/up pair,
which the fixture intercepts and suppresses before ordinary form accounting. An
awaited, read-only zero-delay renderer turn must then report exact trusted
down-then-up evidence, zero Shift keypress/repeat evidence, unchanged form state
and unchanged ordinary counters before the corresponding Enter. Missing,
partial, reordered, late or extra Shift evidence fails the run; the sentinel, Enter
and literal Ada insertion are never retried. The first Enter must produce the
invalid form state; an ordered `filled` document retains Ada and the second
sentinel before the next Enter; the second Enter must produce `Saved Ada.`; and
the existing full accessibility and denied-origin oracles remain unchanged. The
Shift observation proves only that this synthetic key reached the renderer at that
moment; the settle window is not promoted to a general WebView2 readiness
guarantee. See the
[checkpoint](../../../../../docs/development/cs3-native-dom-checkpoint.md).
An exit-zero controller receipt means receipt/cleanup completion, not successful
browser interaction. Always inspect its outcome and qualification fields.

Native execution is user-authorized only as the full, exact reviewed plan. A new
independent root review must pass after the build and before invoking the staged
controller:

```powershell
pwsh -NoProfile -File D:\code\Github\vcp\artifacts\cs3-webview2-NEW\Invoke-NativeProbe.ps1 `
  -Mode webview2-dom -Execute -ExpectedInputsSha256 <reviewed-build-manifest-sha256>
```

Each execution is a new diagnostic attempt; retain failed outcomes. Do not
substitute inputs or treat build success as qualification. Reconciliation uses
the staged script's bounded `reconcile` mode
and an exact retained receipt; follow its fail-closed instructions rather than
deleting an unresolved profile.

The fixed Evergreen runtime is serviced in place. Exact launch-time and
before/after hashes and inventories do not prove continuous runtime immutability.
`--edge-webview-no-dpi-workaround` is a development diagnostic for the documented
shell-launch issue only; it is not a sandbox or capability switch and does not
establish general compatibility. This probe supplies bounded diagnostic evidence
only. It does not qualify browser/server isolation, visual behavior, or CS-3.
