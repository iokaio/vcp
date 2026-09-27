# CS-3 WebView2 diagnostic source

Local Hyper-V staging variant (not yet qualified): based on commit
`9a27a2cf6cc4fa690a5d05b330ca0df146cc5031`, now pins the guest's observed
WebView2 `153.0.4234.48` executable SHA256
`65afdc3965a6d1c4ccd5b47801fec8a16db15613d35c8e3a6d1fb6c0da970eea`.
The earlier SDK's package version was not recorded, so this new diagnostic uses
the explicitly versioned Microsoft.Web.WebView2 NuGet package `1.0.4191.47`,
package SHA256 `f492bbf547d0da329553b6727435b677579b1e9f91cc9e4a1ad029366d5f23d0`.
Core (`lib/net462`) SHA256 is
`e6f54c8ce208e3797c427d01ad671b47cb25abc85604753d6ec2546d0ffef550`;
loader (`build/native/x64`) SHA256 is
`c66e4a92fdc7a216118e43b7a5024ea2200e8c43f9310bf20d96a0084f82c5bc`.
Both vendor LICENSE and NOTICE accompany staged inputs. This is a fresh source
variant, not a reproduction or pass of the earlier diagnostic. The original
SDK pins below are historical and remain in Git history; compatibility and a
new browser-free build must be checked before reviewing any native launch.

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

The fixed form records passive script readiness and trusted key/submit counters;
read-only snapshots are checked independently by the parent. Native forward
focus is not a claim of CDP Tab input. The current source-bound build runs a
separate input-routing diagnostic: read-only CDP target identity, native focus
observations, an optional fixed text-insertion control, then Enter. The current
compiled control omits text insertion and waits 100 ms after focus and at the
pre-key observation stage, with no post-key delay, within the unchanged
20-second lifetime. These source-bound timing controls are not a production
readiness strategy. The [routing report](../../../../../docs/development/cs3-input-routing-diagnostic.md)
retains the successful and unsuccessful timing comparisons.
`input_diagnostic_observed` means all diagnostic records were retained, not that
keyboard input or any full DOM/AX/origin oracle passed. The prior full-form
assertions remain separate and are not relaxed. See the
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
