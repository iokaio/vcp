# P2-01 repository/context increment

On September 18, 2026, the native context runner passed all 19 contracts:
seven repository observations, six context cases, four domain regressions and
two protocol regressions. P2-01 remains `in_progress`: the actual retained
model-request integration and provider compatibility are subsequent work.

## Executed evidence

`pwsh -NoProfile -File scripts/test-context.ps1` exited 0 on Windows 10.0.26200,
NTFS, Rust 1.98.0, MSVC 14.50.35717 and Git 2.51.0.windows.1. It verified Codex
source and ran locked, offline native Cargo tests. The recorded input hashes
cover source, manifests, runner, lockfile and reconstructed inventory. Synthetic
fixtures and captured text contain no private workspace history or credentials.

| Boundary | Observed result |
|---|---|
| Native file observation | Open handles deny concurrent writes/replacement; later user edits invalidate versions; ADS/escape paths reject |
| Reparse scope | A real Windows junction to an outside marker is excluded without reading its contents |
| Discovery | Ignore/generated/binary/oversize/entry/depth exclusions remain explicit; newly created ignore files invalidate dependencies |
| Instructions | Parent/root/nested AGENTS.md applicability stays per path; missing nested instruction creation invalidates preparation |
| Git | Actual staged CRLF, unstaged and untracked changes preserve working bytes and index; malicious clean filter never executes |
| Workspace manifest | Untracked byte changes alter the manifest even when porcelain status does not change |
| Selection | Hostile evidence keeps its trust label; smaller envelopes retain mandatory state or fail; overlapping ranges deduplicate exactly |
| Tool conversation | Complete pairs stay together; unfinished pairs and unsupported tool envelopes reject |
| Sealing | Changed revisions, source freshness, schemas, model envelope or manifest invalidate the seal |
| Canonical bytes | Both SQLite and files stores reopen captured artifacts; exact selected bytes verify and forged partial views reject |

Local evidence is `artifacts/context/4fe36d13-9b0e-48be-97f8-b344fa815d8a/manifest.json`,
SHA-256 `afa77b7557a9a4a1d5d19387e2930972a20c70afafbfdb0fda5d3f9bf2bac72a`.
The contracts log SHA-256 is
`78e2bf6123ac639a58766b44ea2ebcfd950fa4e99d0e75533274de75103113c2`.
Raw manifests/logs remain ignored local outputs; the guide supplies reproduction
commands without those outputs.

Independent reconstruction from Codex `3d3ae4965ab370217e871b3a7f0d15589557ee4b`
plus ordered patches matched all 7,938 committed source files. Result aggregate:
`bba4dbd64fdfa5a46742fb3f1540912a59cfd3eb1e4a350c96ccb6518dcc903f`.
Patch 0014 SHA-256:
`cb6e0fd0a427f1fd6497082c953db5b91e23fdfd14cc6d0067ed00d4af568b85`.
All external Cargo package entries remain identical. Original workspace packages
are assigned to 32 boundary groups with 67 concrete seam anchors.

## Remaining acceptance

The [source guide](../development/p2-context.md) records current API bounds and
unsupported Git layouts. These tests do not establish model-role conversion,
provider tokenization, policy admission, actual network send fencing, product
coding completion, compaction or console-close recovery. No live provider call
was made. P2-02 through P2-08 retain their own acceptance gates. No optional
repository-map benefit is claimed without a future pinned comparison.
