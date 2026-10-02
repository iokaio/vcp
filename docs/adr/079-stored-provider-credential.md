# ADR-079 — Opt-in stored OpenRouter key for interactive sessions

Date: October 2, 2026. Status: accepted by explicit owner direction; implementation in progress.
Owning item: BETA-12 in the [release plan](../release-planning/00-release-plan.md).
Extends [ADR-076](076-signed-beta-qualification.md) and [ADR-077](077-guided-cli-setup.md).

## Context

The provider key came only from `OPENROUTER_API_KEY` in the process
environment, so every new terminal needed the key re-entered before
`vcp setup provider` or `vcp run`. During guided setup planning the owner chose
opt-in storage in Windows Credential Manager. ADR-076 keeps credentials out of
command arguments, committed files, profiles and evidence. Existing automation
also relies on a simple rule: a process launched without the variable cannot
make a paid call. The delegation runner's preflight, for example, removes the
variable and expects refusal.

## Decision

- `vcp setup credential store` reads the key from a hidden console prompt and
  writes one generic Credential Manager entry, `VCP/OpenRouter/v1`. The entry is
  per Windows user, persisted on the local machine (it does not roam) and
  protected by the operating system. `vcp setup credential remove` deletes it.
  `vcp setup credential status` reports only whether each source is present.
- There is still no `--api-key` argument, and the key never enters a profile,
  the data folder, JSONL, rendered output or diagnostics. In-process copies are
  zeroized on drop, and the system buffer is overwritten before it is freed.
- **Precedence.** A nonempty `OPENROUTER_API_KEY` always wins.
- **Scope.** The stored entry is consulted only for an interactive terminal
  session: text format, no `--non-interactive` or `--control-stdin`, and stdin,
  stdout and stderr all consoles. JSONL, redirected and automation runs use
  only the variable, so withholding it still prevents provider calls there.
  The editor keeps passing its own credential over the private bootstrap pipe.
- Hidden entry disables console echo only for the read. The original mode is
  restored by a guard and by a temporary control handler on Ctrl+Break or
  console close. Ctrl+C or an empty line cancels without storing anything.
- Qualification tooling binaries keep requiring the variable explicitly.

## Consequences

- An attended `vcp setup provider` or `vcp run` works in a new terminal after
  one `vcp setup credential store`.
- `doctor` and `setup check` report the active source. `doctor` reports the
  stored entry as usable in interactive sessions only.
- A stored key remains readable by any process running as that Windows user.
  This is the same exposure as the variable in that user's processes; users who
  do not accept it simply do not store the key.
- Removing the entry is the user's responsibility. Uninstalling VCP does not
  remove it.
