# Local Windows installer loop

The BETA-06 local builder makes an unsigned installer directly from the current checkout, including recorded uncommitted changes. It uses the cached production Cargo target and does not invoke CI, commit, push, or publish. The reviewed release builder (`scripts/build-setup.ps1`) remains a separate workflow.

The local builder automatically selects the next unused numeric patch version and synchronizes the native app, SDK, VS Code extension, lockfiles, `release/internal-beta.json`, and upstream patch provenance before compiling. An already advanced, synchronized unused version is retained. Reusing a produced version for different bytes is prohibited. Each build reserves `artifacts/local-setup/<version>`, including builds with a custom `-OutputRoot`; an interrupted build retains its evidence and the next attempt uses a new version. Only one local setup build can use a checkout at a time.

From the repository root in PowerShell 7 on Windows, with the documented native production toolchain installed:

```powershell
.\scripts\build-local-setup.ps1
.\scripts\install-local.ps1
```

The builder automatically downloads the Inno Setup compiler pinned in `release/internal-beta.json`, verifies its SHA-256 before use, and caches it under `artifacts/build-tools/inno-setup`. Subsequent builds reuse the verified cache. To use an existing download offline, pass `-CompilerInstaller C:\tools\innosetup-6.7.3.exe`; the same hash check applies. `-Jobs` defaults to 4. The compiler is extracted locally, and the resulting setup EXE does not download product prerequisites. The second command selects the latest completed local candidate, verifies its installer hash, and opens the normal interactive installer. Use `-WhatIf` to inspect its selection.

The builder also accepts `-NativeResult`, `-BuildReceipt`, and `-Launcher` together for an already built matching local production package. This mode preserves the recorded version and refuses an existing setup reservation; omit these inputs to build a new automatically versioned candidate. All recorded source inputs must still match the checkout. It rejects qualification binaries, version drift, changed engine/launcher bytes, and changed build tools. Successful output is a `setup-result.json` under the version directory; adjacent `native-result.json` and `package/build-receipt.json` can supply the matched VSIX packager.

The local manifest explicitly records `local_candidate` provenance and unsigned status. It does not assert reviewed source, CI success, release qualification, or installed-product validation. Cached builds provide a faster local test loop; a release still requires its existing clean-source construction and qualification gates. Installing the candidate and running the CLI scenarios remain separate verification steps.
