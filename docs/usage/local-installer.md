# Local Windows installer loop

The BETA-06 local builder makes an unsigned installer directly from the current checkout, including recorded uncommitted changes. It uses the cached production Cargo target and does not invoke CI, commit, push, or publish. The reviewed release builder (`scripts/build-setup.ps1`) remains a separate workflow.

Before each new candidate, increment the synchronized numeric version in the native crates, SDK, VS Code extension, lockfiles, and `release/internal-beta.json`. Reusing a produced version for different bytes is prohibited. The local builder reserves `artifacts/local-setup/<version>` and refuses to overwrite it, including an interrupted build; retain failed evidence and increment before retrying.

From the repository root in PowerShell 7 on Windows, with the documented native production toolchain installed:

```powershell
.\scripts\build-local-setup.ps1 -CompilerInstaller C:\tools\innosetup-6.7.3.exe -Jobs 4
.\install-local.ps1
```

The compiler installer must already exist and match the SHA-256 pinned in `release/internal-beta.json`. It is extracted locally without downloading prerequisites. The second command selects the latest completed local candidate, verifies its installer hash, and opens the normal interactive installer. Use `-WhatIf` to inspect its selection.

The builder also accepts `-NativeResult`, `-BuildReceipt`, and `-Launcher` together for an already built matching local production package. All recorded source inputs must still match the checkout. It rejects qualification binaries, version drift, changed engine/launcher bytes, and changed build tools. Successful output is a `setup-result.json` under the version directory; adjacent `native-result.json` and `package/build-receipt.json` can supply the matched VSIX packager.

The local manifest explicitly records `local_candidate` provenance and unsigned status. It does not assert reviewed source, CI success, release qualification, or installed-product validation. Cached builds provide a faster local test loop; a release still requires its existing clean-source construction and qualification gates. Installing the candidate and running the CLI scenarios remain separate verification steps.
