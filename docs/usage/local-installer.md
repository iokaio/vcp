# Local Windows installer loop

The BETA-06 local builder makes an unsigned installer directly from the current checkout, including recorded uncommitted changes. It uses the cached production Cargo target and does not invoke CI, commit, push, or publish. The reviewed release builder (`scripts/build-setup.ps1`) remains a separate workflow.

For faster CLI-only testing with the current version and no installer or VSIX,
use [the local rebuild and deploy script](local-deploy.md).

The local builder automatically selects the next unused numeric patch version and synchronizes the native app, SDK, VS Code extension, lockfiles, `release/internal-beta.json`, and upstream patch provenance before compiling. An already advanced, synchronized unused version is retained. Distributable candidates must not reuse a produced version for different bytes. Each build reserves `artifacts/local-setup/<version>`, including builds with a custom `-OutputRoot`; an interrupted build retains its evidence and the next attempt uses a new version. Only one local setup or deployment build can use a checkout at a time.

From the repository root in PowerShell 7 on Windows, with the documented native production toolchain installed:

```powershell
.\scripts\build-local-setup.ps1
.\scripts\install-local.ps1
```

The builder automatically downloads the Inno Setup compiler pinned in `release/internal-beta.json`, verifies its SHA-256 before use, and caches it under `artifacts/build-tools/inno-setup`. Subsequent builds reuse the verified cache. To use an existing download offline, pass `-CompilerInstaller C:\tools\innosetup-6.7.3.exe`; the same hash check applies. The compiler is extracted locally, and the resulting setup EXE does not download product prerequisites. The second command selects the latest completed local candidate, verifies its installer hash, and opens the normal interactive installer. Use `-WhatIf` to inspect its selection.

Local setup builds automatically choose a Cargo job count based on CPU and memory capacity: leave one logical processor free, budget 4 GiB per job, and cap the default at 32 jobs (with a minimum of one). Override it with `-Jobs 23`, for example; local builds accept 1–256 jobs. The production compilation cache is retained between candidates. More jobs can improve dependency compilation, while linking and installer packaging may still limit total build speed. Qualified release builds retain their existing 1–16 job limit and default of two.

If an older in-process build left VCP's exact `CARGO_ENCODED_RUSTFLAGS` value in the calling PowerShell session, the local builder temporarily removes it for production compilation and restores the caller's value afterward. Custom flags and other build overrides remain rejected. The production recipe restores its environment changes on success or failure; invoking `build-production.ps1` directly still rejects inherited compiler overrides.

The builder also accepts `-NativeResult`, `-BuildReceipt`, and `-Launcher` together for an already built matching local production package. This mode preserves the recorded version and refuses an existing setup reservation; omit these inputs to build a new automatically versioned candidate. All recorded source inputs must still match the checkout. It rejects qualification binaries, version drift, changed engine/launcher bytes, and changed build tools. Successful output is a `setup-result.json` under the version directory; adjacent `native-result.json` and `package/build-receipt.json` can supply the matched VSIX packager.

The local manifest explicitly records `local_candidate` provenance and unsigned status. It does not assert reviewed source, CI success, release qualification, or installed-product validation. Cached builds provide a faster local test loop; a release still requires its existing clean-source construction and qualification gates. Installing the candidate and running the CLI scenarios remain separate verification steps.
