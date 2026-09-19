# P2 authority acceptance assessment

Status: P2-03 complete for the implemented native authority boundary on
September 19, 2026. Installed CLI behavior and general OS sandbox qualification
remain with P3 and P8 respectively.

The authority core and native brokers share one prepared-operation contract.
Current workspace trust, host/user denials, operation identity, required isolation,
scoped grants and preset rules are checked before effects. Canonical questions
bind actor, operation, revisions and expiry; their answers do not resume held
work. Model text, project instructions and process output grant no authority.

## Implemented presets

All rows require current authority, trusted workspace, supported required
isolation and no applicable denial. Existing valid scoped grants are evaluated
before preset allowance, except that planning rejects mutations and processes
even with a grant. Money limits remain separate.

| Mode | In-scope local reads | In-scope local edits | Native processes and opaque effects |
|---|---|---|---|
| Plan | Automatic | Denied | Denied |
| Ask | Automatic | Scoped grant or durable question | Scoped grant or durable question |
| Workspace (library default) | Automatic | Automatic within configured workspace roots | Scoped grant or durable question |
| Autonomous | Automatic | Configured automatic effect ceiling, or scoped grant/question | Configured roots and complete effect ceiling, or scoped grant/question |

The preset name alone never authorizes an unregistered root. A process with an
unknown effect closure is classified as opaque; declared inputs cannot make a
relevant trusted denial disappear. Exact grants bind the complete prepared
operation. Configured grants still match invocation, arguments, schema, effects,
roots, paths, required isolation and resource limits.

## Acceptance mapping

| Boundary | Evidence and independent observations |
|---|---|
| Presets and exact matching | Six `vcp-policy` contracts exercise the four modes, trusted-denial precedence, changed approval fields, scope, expiry, revocation, opaque shells and missing isolation |
| Durable decisions | Canonical engine contracts run on SQLite/files, including JSONL waiting receipts, actor/epoch binding, idempotent repeated answers, conflicting/stale answers, pause and old-owner reopen |
| Native file broker | `native_file_broker_enforces_current_policy_approvals_and_source_versions` observes file bytes and absent new files after rejected stale or denied operations, alongside granted execution |
| Native process broker | `process_broker_observes_authority_argv_limits_and_native_tree_stop` observes argv/markers, changed executable/input/profile rejection, plan denial, durable ask, deliberate resume and grant reuse without another question |
| Trusted host ceiling | `host_tool_denials_survive_user_policy_and_grants_without_native_dispatch` retains host denials under permissive user policy/grants, including named and opaque operations |
| Active authority change | Retained stream/process traces stop owned work and pause the tree on policy/trust/grant/binding/steering changes while preserving late usage and uncertain effects |
| Retained context/tools | Request and tool admission reject source/steering/policy changes and invented/replayed tool identities; the verification wrapper cannot bypass named workflow ceilings |

The field-by-field identity matrix is a pure contract. Native tests independently
observe selected file/process rejection and execution cases; they do not repeat
every pure variant at every broker boundary. Headless evidence covers the shared
JSONL handler, not the future installed CLI's presentation or exit-code contract.

## Current qualification

`pwsh -NoProfile -File scripts/test-policy.ps1 -Jobs 2 -TargetRoot
C:/code/Github/vcp/artifacts/context-continuity-target -OutputRoot
C:/code/Github/vcp/artifacts/policy-acceptance` passed all 47 authority/foundation
contracts, exit 0. It ran from an isolated checkout of main `33790fe` with only
documentation changes. Manifest:
`artifacts/policy-acceptance/bfeca760-e209-433f-8dbc-ed52a9faa086/manifest.json`,
SHA-256 `e2a5e65849d88b91eed202a37e590210faff07d0b021577c8b99596f5903774f`.
Native environment: Windows `10.0.26200`, NTFS, MSVC `14.50.35717`, experimental
Rust `1.98.0`. The original compiler pin is unchanged.

The native host evidence is recorded in the
[continuity qualification](p2-context-compaction.md): 44 integration contracts,
85 foundation contracts, recovery and 29 retained lifecycle regressions. Every
recorded input hash was compared with this checkout: all 132 integration inputs,
131 foundation inputs and 41 lifecycle sources in each recovery/lifecycle
manifest matched. Those completed results apply to unchanged source; the large
native host suites were not rerun solely for this documentation assessment.

The governing manifests are integration `a582a31f-07f4-47b0-8c34-723ce8079671`,
foundation `0504555c-cd4a-4666-8464-2b0c2c969edd`, recovery
`5f6f9943-e458-4fd1-98a4-03a82575996a` and lifecycle
`88817a53-7c68-4d28-bc97-2dc23bbea887`; the linked report records their full paths,
commands and SHA-256 identities. No paid provider call was used.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed, exit 0, with an
isolated output root. Manifest:
`artifacts/policy-fast/5e643cce-c746-4bd9-9732-e896ad72787d/manifest.json`,
SHA-256 `7338c56df5f80e5158720d247a84775483e7966c6bcfeb4ff4249f11aef5c1c6`.

## Owning boundaries

P2 authority does not advertise general Windows filesystem or network isolation.
Native process tests use explicit reduced-isolation profiles; a requested
unsupported restriction rejects instead of silently weakening enforcement.
P8-01 owns broader sandbox qualification. P3 owns the installable CLI and terminal
UX. MCP and delegated-work adapters must preserve the same authority contract
when their owning tasks integrate them. Their absence does not establish support
for those future adapters.
