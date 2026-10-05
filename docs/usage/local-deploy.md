# Rebuild and deploy the local Windows CLI

For the BETA-06 development loop, rebuild the current checkout, including
uncommitted code, and deploy the native files to an existing VCP installation:

```powershell
# From the repository root, in PowerShell 7:
.\scripts\build-local-deploy.ps1 -Jobs 16

# From the scripts directory:
.\build-local-deploy.ps1 -Jobs 16
```

If you already ran `build-local-setup.ps1`, deploy its latest completed native
payload directly, without compiling again or opening the installer:

```powershell
# From the scripts directory:
.\build-local-deploy.ps1 -UseLatestBuild
```

This selects the highest completed local setup version and checks its native
archive, build receipt and payload files before activation. It preserves that
build's version and does not require the current checkout to match the older
build. Use `-BuildRoot <directory>` with `-UseLatestBuild` for custom build output.
Use `-WhatIf` to preview selection without changing the installation.

This explicitly requested local testing workflow keeps the current source
version unchanged. It does not build an installer or VSIX, reserve a new
candidate version, sign files, commit, invoke CI, or publish. Distributable
installer and VSIX candidates still require a new synchronized version and
their normal release checks.

The script uses the locked Rust 1.95.0 production build without qualification
features and retains `artifacts/codex-target` between builds. It requires the
existing Windows x64 native toolchain, Git, Node, and cached Cargo dependencies.
Node runs repository validation helpers; npm and the TypeScript build are not
invoked. Inno Setup is not needed. The default Cargo job budget matches the
local setup builder; `-Jobs` overrides it.

The installed application directory is selected from VCP's current-user or
machine uninstall registration, falling back to the `vcp` application on PATH.
If more than one registered installation exists, select the intended one:

```powershell
.\scripts\build-local-deploy.ps1 `
  -InstallRoot "$env:LOCALAPPDATA\Programs\VCP" -Jobs 16

# Preview discovery and the unchanged source version without building or copying:
.\scripts\build-local-deploy.ps1 -WhatIf
```

`-InstallRoot` is the installed application directory containing the launcher
`vcp.exe` and the `engine` directory. Existing ownership and data-scope records
must match. Deploying to an all-users installation requires a shell with write
access to its installed directory.

Close running VCP CLI processes and editor connections before deployment. The
script builds both the engine and launcher, stages current built-in skills,
notices, usage guides, model specifications and helpers, and verifies the
copied files against the build receipt. A temporary native ZIP lets the existing
upgrade helper perform its normal hash, compatibility and state checks; this
ZIP is a development payload, not an installer or published candidate.

The upgrade retains the previous engine release and atomically selects the new
engine. The script also replaces the application launcher, verifies its selected
engine and unchanged data directory, and checks the actual executable version.
It preserves the installation's registered maintenance scripts, setup notices,
PATH, uninstall registration and protected data. Failed deployment checks
restore the prior launcher and active pointer when possible; backups and build
evidence remain under `artifacts/local-deploy/<run-id>` for recovery.

After a successful deployment, invoke the installed launcher for CLI tests:

```powershell
vcp --version
vcp --resolve-installation
```

`deploy-result.json` records the deployed native path and hash identity, unchanged
numeric version and previous-file backup paths. Tests that use an explicit
engine path must use the newly selected path rather than a retained old release.
Windows installed-app version metadata and an already installed VSIX keep their
original installer/package identity during this development loop.

Leave these changes uncommitted while testing. When the CLI scenarios are
satisfactory, the separate [local installer workflow](local-installer.md) or
reviewed release workflow produces a newly versioned candidate. Local deployment
does not provide signing, installed-product qualification or release evidence.
