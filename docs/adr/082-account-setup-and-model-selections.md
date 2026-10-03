# ADR-082 — Per-user setup and retained model selections

Status: accepted October 2, 2026 for BETA-03D. Owner preference: balanced quality
and cost, with project configuration optional.

## Decision

Interactive bare `vcp` starts setup when the current Windows user's account
completion record is absent. After success it prints exactly the normal
help. Explicit help is parsed before account state, workspace checks, credential
access or setup. Redirected bare invocation also prints help; it never opens a
wizard or infers permission to spend. Existing task discovery is explicit:
`vcp workspace discover`.

Account preferences and setup state live under `%LOCALAPPDATA%\VCP\account`,
independent of install location, current directory, selected project and
`--data-dir`. Records reject repository/sync/network/redirected paths and use
bounded, atomic publication. A per-user setup lease prevents simultaneous
interviews. Completion is published only after the accounted connection test
and requested configuration succeed. Project trust remains an explicit opt-in.

The interview presents balanced quality/cost, offers other sets by maker or
coding-project type, shows role assignments and ordered alternatives, and allows
customization. These sets are owner-selected preferences, not comparative
quality or empirical role qualification. The fresh metadata and adapter
compatibility contract in ADR-081 determines candidate eligibility. Unavailable
members are reported; no role can silently escape the selected set.

`vcp models` opens an attended chooser for account or project preferences, or
reports the effective preferences when redirected. Its list,
show, select, customize and budget subcommands change future tasks only. New
tasks use explicit profiles before project overrides and account defaults.
Each accepted task retains its provider and role selections. Resume and fork
retain these model selections while loading current trust, tool and policy
ceilings normally. Recorded role assignments and candidate order remain fixed;
current trusted privacy, input/output and retrieval restrictions can only narrow
their eligibility. Account and project model preferences still affect future
tasks only. A retained fixed model fails closed when it cannot enforce a new
privacy or routing resource restriction. Fresh metadata may renew the original
exact endpoint; it cannot silently replace the task's chosen model or provider.

Owner-assigned roles use an explicit routing lane with no invented quality
scores. Selection tries eligible assigned candidates in order, preserving
capability, context, data policy and canonical budget admission. Gateway fallback
stays disabled. If none qualify, execution stops with a request to the owner to
change the selection; an outside model is never automatically admitted.
Empirical optimizer escalation cannot be combined with owner assignments.
Retryable availability failures can try another eligible candidate assigned to
that same role, within the task's captured retry, request, deadline and money
limits. A submitted failed request retains its full uncertain liability; a
fallback needs a separate reservation from the actual remaining balance. The
failed identity is excluded from that retry chain. Authentication, protocol and
capability failures do not gain a new retry path. New wizard profiles use the
existing bounded two-retry default; the setup connection test still has none.

Credential source selection is stored separately as nonsecret account metadata.
An explicitly designated environment variable is the only source in that mode;
missing, empty, invalid Unicode and malformed values never fall back. Explicit
stored mode uses the attended session or Windows entry even if an environment
key exists. Legacy default mode retains environment-first precedence.
An attended terminal can use a session key or an opt-in Windows Credential Manager generic entry,
`VCP/OpenRouter/v1`, stored locally for the current user without roaming.
Hidden input restores console mode after entry/cancellation. Arguments,
profiles, model preference records and logs never contain the key. Secret
wrappers wipe their owned buffers; transport and OS allocations have separate
lifetimes. Redirected, JSONL, control-stdin and noninteractive commands consult
only an environment credential: the designated name or `OPENROUTER_API_KEY`
for stored/default mode. Setup pending state stores
a private credential digest to prevent reusing another credential's successful
connection report; the digest is not a credential or an authentication grant.

Any presence of `VCP_DENY_PROVIDER_CREDENTIALS` disables provider credential
access before account selection, environment, session or protected-store lookup.
Offline preflights set this marker and require the explicit refusal receipt;
removing only `OPENROUTER_API_KEY` cannot exclude a designated alias.

## Consequences

Account setup can succeed without trusting a project. Running a task still
requires a trusted workspace profile. Default task budgets and connection-test
budgets are separate, explicitly displayed caps. Conservative reservation can
exceed the eventual charge for a tiny prompt; setup explains the bound before
asking for test authorization. No paid validation campaign, deployment or
release is authorized by this implementation decision.
