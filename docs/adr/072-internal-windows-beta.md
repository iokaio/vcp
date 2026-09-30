# ADR-072 — Internal Windows beta distribution

Date: September 30, 2026. Status: selected for implementation; qualification pending.
Owning item: BETA-01 in the [release plan](../release-planning/00-release-plan.md).

## Context and authority

The owner requested implementation of the release plan to prepare VCP for an
internal beta and manual testing. This selects an internal audience; it does not
authorize public publication, paid provider evaluation, certificate purchases or
waive the release invariants in [ADR-018](018-release-acceptance.md). The original
review and [Orca review](../release-planning/01-orca-review.md) are retained. Orca
feature suggestions do not expand the beta implementation scope.

## Selected mechanism

Use the `internal-beta` channel with native product version `0.2.0-beta.1` and
numeric SDK/VSIX version `0.2.1`. The [channel input](../../release/internal-beta.json)
is the packaging authority. The VSIX is a downloadable Windows x64 pre-release;
Marketplace ownership and credentials are not prerequisites for local installation.
Product versions do not change canonical store, configuration or wire formats.
Same-source qualification successors remain distinctly labeled fixtures and cannot
stand in for a supported upgrade between distinct production builds.

Produce `vcp-0.2.0-beta.1-windows-x64-unsigned-setup.exe`, a matching portable ZIP
and `vcp-local-0.2.1-win32-x64.vsix`. Candidate manifests connect the exact source,
build receipt, payload inventory and all final hashes. A clean checkout and an
explicit reviewed source commit are required for release-mode assembly. That
selection is an input to the release gate, not proof that a human approved it.

Use Inno Setup 6.7.3 as the per-user setup shell around VCP's existing verified
payload and side-by-side lifecycle. Pin its download digest and source revision
in the channel input; preserve its notices and audit its shipped components in
BETA-05. The pinned [license](https://github.com/jrsoftware/issrc/blob/4adf37ed7f3fd2bd11c6836ba056e3de170fbabf/license.txt)
permits use and redistribution with its stated notice and origin conditions.
Use current-user uninstall registration, an explicit data-root choice, optional
shortcut/PATH integration and a stable launcher that validates the active release.
PowerShell 7 remains an explicitly diagnosed user prerequisite for the existing
lifecycle and model scripts; neither Node nor Rust is a user install prerequisite.
The VSIX keeps explicit User settings for the installed engine and data directory.

An MSI rewrite or bespoke installer UI would duplicate the existing lifecycle
without improving this increment's acceptance. An archive alone does not meet
the requested Windows installation experience. No service, auto-updater or
automatic helper/model acquisition is introduced.

## Signing and support envelope

Build an explicitly **unsigned internal candidate**. This is an implementation
choice for the requested internal beta, not permission to distribute the finished
candidate. Checksums identify bytes; they do not authenticate a publisher. Record
unsigned status on the artifact pair, installation instructions and scorecard.
Do not advise bypassing organization policy. Public distribution or introduction
of signing requires a separate decision and qualification of the final transformed
bytes with an unsigned-build-to-signed-payload receipt.

The qualification target is native Windows x64 with local NTFS paths and VS Code
`1.138.0`, using both Files and SQLite canonical stores. The exact Windows build,
patch level, CPU, memory and runtimes must come from each candidate's execution
record. Historical Windows `10.0.26200.0` results are not current evidence; the
implementation workstation reports `10.0.26300.0` and is not a clean installation.
Do not advertise a Windows 10/11 version range or hardware minimum before tests.

Excluded from this increment: ARM64/x86, non-Windows hosts, remote/WSL/container
workspaces, Marketplace delivery, unattended auto-update, cross-format migration,
arbitrary binary downgrade, and physical-device/power-loss durability claims.
Independent-machine recovery, clean Windows, network-denied local memory,
full-volume recovery, real editor interaction and owner quality assessment remain
visible qualification rows. They are not passed or waived by this decision.

## Evidence and external gates

BETA-09 must execute the [qualification matrix](../release-planning/00-release-plan.md#5-required-beta-qualification-matrix)
against final bytes. Retain source/artifact identities, commands, environment,
pass/fail/not-run disposition, both-store results and sanitized durable logs.
Any changed packaged byte requires new hashes and affected requalification.
Unauthorized effects, record/edit loss, unresolved accounting incorrectly cleared,
secret disclosure or false completion stop acceptance.

External inputs are a clean standard-user Windows environment on the declared
build, actual VS Code interaction, separately admitted live-provider budget and
credentials when those cases run, and factual owner acceptance of the final
scorecard. BETA-11 requests distribution authorization only after the candidate
and its evidence are reviewable. No certificate or Marketplace input is required
for constructing this unsigned internal candidate.
