# P8-04 Windows distribution candidate

`scripts/package.ps1` assembles an unsigned Windows ZIP from an explicit native executable, validated built-in skills, notices, and optional runtime files. Its manifest hashes every payload and records source, target, compatibility and model provisioning metadata. A supplied `vcp-local-build/1` receipt must bind the exact executable digest; without it, provenance is explicitly unverified. The archive digest is recorded beside the ZIP in `result.json`.

```powershell
pwsh -File scripts/package.ps1 -Executable artifacts/vcp.exe `
  -BuildReceipt artifacts/build-receipt.json -OutputRoot artifacts/p8-distribution
```

The package includes standalone PowerShell 7 installer and model-provisioning scripts. Installation uses PowerShell/.NET and does not require Node or a source checkout. Extract `tools/package-install.ps1` from the candidate and invoke it with explicit, disjoint installation and data roots:

```powershell
pwsh -File package-install.ps1 -Action Install -PackageZip vcp-windows-unsigned.zip `
  -InstallRoot 'C:\VCP' -DataRoot 'C:\Users\me\AppData\Local\VCP'
```

`active.json` identifies the selected executable under `releases/<archive-sha256>`. Start that `vcp.exe` with the same explicit `--data-dir` and intended `--workspace`. `doctor --vault <path> --staging <path>` checks separated local, staging and vault paths; a path check is not encrypted recovery verification.

`Upgrade` validates the complete candidate, retains releases side by side and atomically changes the active pointer. `Rollback` selects only the recorded previous compatible release. `Uninstall` validates the installation ownership marker, all retained payloads and ordinary paths before removing installation files. Workspace, history, key and vault locations remain outside that root. Unexpected files, redirects, locked payloads and mismatched data-root ownership are explicit errors. The deterministic `VCP_PACKAGE_INSTALL_FAULT=before-activation` test hook leaves the previous release active and permits retry with the candidate.

Compatibility declares `vcp-store/1+replay-base/2`, `vcp-cli-profile/1`, and `derived-index-rebuild-required`. Upgrade and rollback refuse mismatched declared state formats. This candidate does not implement cross-format migrations; those require a validated snapshot and staged restore before activation. Portable history and encrypted vault formats retain their separate contracts.

Model assets are not bundled or downloaded on startup. The package includes the pinned MiniLM specification and its license declaration. Explicit acquisition verifies each asset's byte count and SHA-256 before publishing the file:

```powershell
pwsh -File tools/package-models.ps1 -Action Acquire -Destination 'C:\VCP-models\MiniLM'
pwsh -File tools/package-models.ps1 -Action Verify -Destination 'C:\VCP-models\MiniLM'
```

Use a new destination outside the installation. Failed acquisition retains its partial evidence; it does not silently reuse unverified bytes or spend provider budget. The package has no signing certificate or signature. Any later signing step changes the artifact identity and requires checksums and qualification of those final bytes.

`scripts/package-install.test.ps1` exercises native paths, locks, redirects, upgrade interruption, rollback and sentinel preservation. The exact-artifact smoke command is:

```powershell
pwsh -File scripts/evals/distribution-qualification.ps1 -PackageResult <result.json>
```

It installs the recorded ZIP using its packaged installer, runs the actual CLI with an empty process environment and fresh profile directories, checks first-run diagnosis, and uninstalls while preserving sentinels. It records startup time, peak working set and CPU time. This same-host smoke test does not establish a clean OS installation, second-machine encrypted recovery, live task quality or owner acceptance. Those remain separate P8 qualification rows.
