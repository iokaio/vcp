# ADR-077 — Guided CLI setup and workspace profile selection

Date: October 2, 2026. Status: accepted by explicit owner direction; implementation in progress.
Owning item: BETA-12 in the [release plan](../release-planning/00-release-plan.md).

## Context

A new user could not get from installation to a first task without the manual
walkthrough. Running `vcp` from the home folder failed with a misleading
placement error, every error printed a JSONL record into a text terminal, and
`setup provider` required an exact model, endpoint, price limit, cap and output
directory chosen from the provider catalog by hand. Every later command also
needed an explicit `--config`, because a profile is bound to one workspace and
no default identified it.

The owner asked for a guided CLI with reasonable defaults: Qwen 3.8 for quick
tests, a choice of model sets, step-by-step setup and tests, and switching
between prepared sets. That guidance must not weaken the existing spending,
trust and credential boundaries ([ADR-005](005-autonomy-and-isolation.md),
[ADR-007](007-profiles-and-routing.md), [ADR-076](076-signed-beta-qualification.md)).

## Decision

**Output.** JSONL mode, and text mode with redirected stdout, keep the
versioned record contract unchanged, including exactly one final record after
a failure. With text mode and a terminal on stdout, diagnostics and guidance
replace that record and setup/doctor results render as plain text. Exit codes
do not change. Interactive task output is out of scope.

**Workspace profile selection.** `<data>\profiles\selected\<workspace key>.json`
(schema `vcp-workspace-profile-selection/1`, unknown fields rejected, 64 KiB)
names the workspace's active profile by absolute path. It also maps prepared
model sets to their profiles, so switching between sets needs no schema change.
Precedence is: explicit `--config`, then the selection, then the legacy
`<data>\profile.json`. The pointer grants nothing. Loading still enforces the
profile's workspace binding, trust, expiry and import revision. Only a profile
that loads for that workspace can be selected. Reads and writes reject
redirected directories, and replacement is atomic. `setup profile` creates
into the data folder by default and selects the new profile; `setup select`
selects an existing one. Configuration import still requires an explicit
`--config`.

**Defaults are suggestions, never authority.** Built-in model sets choose
models, endpoints and profile limits only. Every member still passes the live
catalog check and the accounted two-call qualification before use. A missing or
unavailable member fails visibly; nothing is silently substituted or retried.
Research tiers are labels, not quality evidence. Spending amounts are never
pre-filled. Interactive steps show computed minimums from the live catalog and
require a typed amount and a typed confirmation. In non-interactive use, an
explicit `--budget-usd` remains the spending authorization.

Per-role model assignment and stored credentials are separate decisions with
their own records.

## Consequences

- `vcp run`, `resume`, `sessions resume|fork`, `skills list`, `setup check` and
  `doctor` work without `--config` once a profile is selected.
- Existing scripts that pass `--config` and parse JSONL or redirected output are
  unaffected.
- `setup profile` without `--output` now writes under the data folder. With an
  explicit `--output` it still writes there, and in either case it also writes
  the selection pointer.
- Doctor and setup check report which source supplied the profile.
