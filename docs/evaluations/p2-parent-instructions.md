# Explicit parent instruction qualification

This P2-01 increment gives the trusted owner an explicit `AGENTS.md` read grant
for ancestor directories. The grant adds no ordinary file, search, process or
mutation scope. Request assembly, admission, tool dispatch and verification
completion use current instruction sources and absence probes.

The registered native fixture runs six scenarios on SQLite and files/journal:

- Explicit guidance appears in the request; an ungranted parent does not.
- Editing parent guidance after request dispatch prevents the returned read tool
  from executing against stale context.
- Creating a previously absent parent instruction has the same invalidation.
- A named read denial rejects the parent grant before any HTTP request.
- An attempted `../private.txt` read creates no native effect and exposes no bytes.
- A parent edit after verification rejects completion only when that parent was
  explicitly granted. Ungranted parent edits do not affect workspace acceptance.

The fixture also rejects duplicate identities, a foreign workspace identity,
the workspace itself as a parent, and changes to grants after setup is frozen.
Independent native effect records, captured HTTP bodies and final filesystem
bytes establish the behavior; checks do not rely on model prose.

## Executed checks

Native Windows 10.0.26200, NTFS, MSVC 14.50.35717 and experimental Rust
1.98.0 (`88d9e12ae 2026-08-18`) were used. The existing 1.95 selection baseline
and committed upstream source are unchanged.

The focused `vcp-lifecycle` `canonical_host` test
`parent_instruction_grants_preserve_scope_and_fence_edits_and_completion` passed
all 12 store/scenario combinations, exit 0 in 45.12 seconds. Fast checks passed,
exit 0; manifest `artifacts/tests/13be8b7e-4362-413f-82b5-b59906aa209a/manifest.json`.
An initial integration run found a regression in
`provider_send_fence_rejects_changed_source_steering_and_policy_before_billable_admission`:
parent-grant validation was also requiring coding policy on the existing
provider-only context path. Admission now validates only configured parent grants.

The first rebuild could not replace the Windows test executable while the
original suite was still running (`LNK1104`). The owner then requested a stop.
The task's native test process was stopped, and the qualification chain exited
nonzero. Its interrupted manifest is
`artifacts/integration/0a9dc603-d468-4013-9a6e-444f0fadaee8/manifest.json`.
That interrupted attempt is not acceptance evidence.

The corrected stable source subsequently passed
`pwsh -NoProfile -File scripts/test-integration.ps1`, exit 0: all 46 native
library/host/controller/integration/port contracts, both retained process-launch
regressions, the CLI/example build and the seven-request private owner trace.
Manifest
`artifacts/integration/d0eb72e5-7470-4676-955d-c9784cb4c601/manifest.json`,
SHA-256 `dcab9131db98a4ae2f336d6ff0655d73076d8e171072313fe0fce014bdd67232`.
The updated retained-host regression also passed all 87 contracts; manifest
`artifacts/p1/9cb074b7-1073-4ae7-937e-e89f6e2f13d4/manifest.json`, SHA-256
`3d43aa90ac3ad641e80577ed3eda1cc7cba0530cfdab3e94b7423c07b01aaef3`.
The final fast delivery suite passed all eight cases; manifest
`artifacts/tests/53628a7c-b268-49c4-a0a1-013c41ba77e5/manifest.json`, SHA-256
`1bd53d3d64991e1887cbf97b76421e356c7f57a3185a7099c1bd9043574ed0fe`.
No paid provider calls were made.

## Scope

These tests use actual Windows file reads, the retained engine, canonical stores
and a scripted loopback provider. They do not establish an installed CLI,
automatic ancestor discovery, arbitrary outside-root access, Git/index context,
or the remaining P2-01/P2-08 acceptance. Restored text cannot recreate a grant;
reopening requires fresh explicit owner setup. No vendor or dependency changes
are included.
