# Internal beta scope, known limitations and support

This is an experimental Windows x64 beta. The next source version is
`0.2.0-beta.2`; SDK and VSIX use `0.2.2`. Its channel requires timestamped
Ioka LLC signatures on the native engine, launcher, setup and uninstaller.
Signed candidate construction and qualification are in progress; the published
native `0.2.0-beta.1` and VSIX `0.2.1` remain the earlier unsigned-native pair.
The release-pair record and scorecard identify actual artifacts, signature checks
and observed environment. GitHub downloads and the Marketplace listing are live;
availability and code signing do not establish completed owner acceptance or
SmartScreen reputation.

The qualification target is local NTFS workspaces and VS Code 1.138.0. ARM64/x86,
non-Windows, remote/WSL/container workspaces, native auto-update,
arbitrary downgrades and cross-format migration are excluded. No hardware minimum,
general Windows version range or power-loss durability guarantee is advertised.

The next Marketplace package uses `iokaio.vcp`. Earlier packages use
`iokaio.vcp-local` or `vcp.vcp-local`; disconnect and uninstall the old extension before installing the
new identity. Retain native data and User settings, then reconnect explicitly.
Marketplace extension updates do not install or update the native engine; select
the matching native pair and update the executable setting deliberately.

Before manual testing, read every `fail` and `not run` row in the supplied scorecard.
Clean standard-user Windows, final installed CLI/editor interaction, real distinct
version upgrade, independent-machine recovery, full-volume recovery, network-denied
local memory and owner quality evaluation require their own evidence. Unit tests,
developer-workstation runs and synthetic errors cannot fill those rows. Stop using
a candidate that loses acknowledged state or user edits, ignores permissions,
reveals secrets, clears unresolved charges, or leaves uncontrolled child processes.

Operational limits:

- PowerShell 7 must be installed explicitly at its standard Program Files path.
  Optional helper tools and embedding models are separate user prerequisites.
- OpenRouter setup uses paid fixed probes with an explicit cap. Profiles/catalogs
  expire; renew them through the supported commands. Missing observed cost is
  retained as liability, not treated as free work.
- The generated profile permits reads automatically and contains no external
  process checks or MCP tools. Configure and review those deliberately before
  asking VCP to execute a project's test suite.
- Reload and reconnect are observational. Review task state and acquire control
  explicitly; unknown command outcomes must be reconciled before retry.
- Local histories, profiles and model context are plaintext. Coding requests send
  selected context to the configured provider. Local retrieval is not a promise
  that every operation is offline.

## Report a problem safely

Record the release-pair ID, setup/ZIP/VSIX hashes, `vcp --version`, Windows build,
VS Code version, store choice, command name, expected result, actual result and
whether the problem reproduces in a fresh nonsensitive sample workspace. Include
the relevant sanitized error and test receipt identifiers. Omit identifying local
paths when they are not necessary.

Do not upload environment dumps, credentials, plaintext task histories, provider
request/response captures, profile files, repositories, recovery keys, unreviewed
setup logs or database files. Review every attachment locally. Preserve private
original evidence so unresolved effects and charges can be reconciled.

Use the project's internal beta handoff or
[GitHub Issues](https://github.com/iokaio/vcp/issues) for nonsensitive reports.
Report security problems through the private routes in
[SECURITY.md](https://github.com/iokaio/vcp/blob/main/SECURITY.md).
