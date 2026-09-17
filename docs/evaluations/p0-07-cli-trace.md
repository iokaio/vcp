# P0-07 native CLI request and tool trace

Date: 2026-09-17. Five native Windows baseline cases passed against a scripted
loopback provider. This observes the imported Codex engine, not an implemented
VCP product session. No paid provider request was made.

## Inputs and execution

- Codex source pin: `3d3ae4965ab370217e871b3a7f0d15589557ee4b`.
- Source comparison: all 7,937 imported files match the selected inventory.
- Tested executable SHA-256:
  `6ec1f78a97ecc3d3d41a08e42c0ffc781c98557fca409049b7f5466875180442`.
- Native baseline: Rust/Cargo 1.95.0, MSVC target, Windows 10.0.26200;
  [build/import evidence](p0-07-codex-import.md) records native setup.
- Observer: Node.js 24.10.0 on Windows. The exact runner/helper hashes and dirty
  source identity are in the trace manifest.
- Command: `node scripts/upstream/trace-cli.cjs --binary artifacts/upstream/codex-target/x86_64-pc-windows-msvc/debug/codex.exe`.
- Final run: `artifacts/cli-trace/c7a3fc39-771b-41a1-9d56-4e5b9343932c/manifest.json`,
  overall exit 0. Each case retains observed request bodies, process logs and
  command/exit receipts; the synthetic patch remains available for inspection.

| Case | Requests | Child exit | Observed result |
|---|---:|---:|---|
| Completion | 1 | 0 | One expected assistant message and one completed turn |
| Read-only patch | 2 | 0 | Rejection receipt sent back to provider; no synthetic file or successful file-change event |
| Unsandboxed synthetic patch | 2 | 0 | One successful file-change event and exact expected file contents; tool receipt in second request |
| Transient retry | 2 | 0 | One HTTP 503 retry with unchanged input; one completed turn |
| Provider denied | 1 | 1 | HTTP 401 becomes failed turn; no successful completion |

The read-only tool rejection still allows an upstream assistant turn to complete.
The independent file/receipt oracle is therefore necessary: turn completion
alone cannot establish task success. P2-06 must apply VCP's completion contract.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` also passed all seven cases
and 37 regression tests, exit 0. Its manifest is
`artifacts/tests/39489cd8-0f5e-4417-a1d5-5cf2c1744605/manifest.json`.
The four new regressions cover missing native prerequisites, the loopback
provider and independent rejection/completion/file oracles. Documentation links,
task ownership/dependencies, imported bytes and `git diff --check` pass.

## Failed probes and limitations

An initial wrapper probe failed before launching cases because it passed a
missing inventory object; the wrapper was corrected to load and validate the
declared result inventory and fail on differences. The first tool trace then
failed because `workspace-write` was downgraded to read-only with Windows sandbox
support disabled in the isolated configuration, yielding a patch rejection.
Source inspection of `config/src/config_toml.rs::derive_permission_profile`
confirms that downgrade; this experiment did not attempt sandbox provisioning
and cannot diagnose an installation failure. Both evidence directories remain
under ignored `artifacts/cli-trace/`. The final cases explicitly preserve
read-only denial and identify unsandboxed patch execution separately; the
underlying Windows setup/enforcement gap has not been fixed or counted as passing.

The [procedure](../development/native-cli-trace.md) describes environment
isolation and fixture restrictions. A loopback request observer does not prove
absence of unrelated network traffic. The supplied executable's hash is an
identity record, not compiler-input attestation. The synthetic retry carries no
real costs and does not qualify atomic admission or uncertain-usage accounting.
Timeout cleanup is not an in-app pause or durable root/child recovery test.
P0-03/P0-05/P0-08 and the remaining P0-07 qualification remain open.
