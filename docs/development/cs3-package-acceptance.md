# CS-3 native package acceptance

The latest [explicit-target refresh](#explicit-target-package-refresh) passes the
four installation stages and all five installed-skill tests against the exact
rebuilt candidate. Earlier receipts below remain historical, not interchangeable
with the refreshed package identity.

The initial September 28, 2026 current-host qualification installs an authentic main-source
baseline, upgrades to the CS-3 candidate archive, restores the exact baseline by
rollback, and re-upgrades to the exact candidate. Each operation uses the installer
extracted from its package and verified against that package's manifest. The
protected user-data sentinel remains unchanged in all four operations. All six
unqualified candidates remain absent from the 21-skill builtin catalog and installed
assets.

The baseline checkout is main commit `bd350386`; the candidate checkout is based
on `45774bae` with the qualification changes under review. Both use the same local
qualification executable and builtin catalog because their production Rust sources
and builtin assets are unchanged. Distinct archive identities bind their source
metadata. This observes compatible package activation and exact rollback; it is
not evidence of a catalog-version migration, a production release build, signing,
or performance qualification. The package manifest accurately marks the supplied
executable's build provenance as `caller-supplied-unverified`.

| Evidence | SHA-256 |
|---|---|
| Baseline archive | `6de4938fe919c244a751596a243a5e28014e69cdc57e89c719d49d8a53a8a2e4` |
| Candidate archive | `16790f8b90a2d954f74900c1f81177cbf377cb91eb5b6bea77473bbac92c10e2` |
| Executable in both archives | `25c339a79b584acb8766fadde27002ce242c6e9f363efea1bc2889fb7d83ee08` |
| Builtin catalog 1.2.0 | `49d565e2661815558639a07719112e81e5fea37258c114438ed02911c85ba8a7` |
| Packaged installer | `649876111cd2c28564ba5495b220e65b1f7683855d657f3ae8c15c6b4518d4a0` |
| Four-stage installation report | `d2b198c9862166f628fa78a31e276d512b24ec7e1cec6e311f1891e6b30e9948` |
| Installed accepted-candidate baseline tests | `f241d563a445628e387d33548e1d4ab673237b4529ef45e7810b6251c94ba2a1` |
| Installed prospective-candidate tests | `6d11680f231f4aa491663a7a154f66b1477edddf5f13a84543e2b4e679c1b0a3` |
| Final prospective tests with complete post-run identity checks | `ae086af2628541ebc92d3048f2c222ca8de9f962f4ebb1a8d393bad7ff906483` |

The report is retained in the current user's temporary directory under
`vcp-authoring-install-ccf3bd1f-9305-4cd0-919b-79190139fc7c/result.json`.
The exact candidate is retained there for native acceptance. The package results
are in ignored `artifacts/cs3-package-baseline-results` and
`artifacts/cs3-package-current-results`. The initial baseline worktree attempt
failed on Windows path length; a separate clean checkout with Git long-path
support completed. That failed checkout was not used as package input.

The all-six terminal revocation regression passes on the local native build. Each
candidate requires explicit source registration and activation, then is disabled
through terminal controls. Reopening canonical state verifies that no skill is
active, the exact candidate is disabled, and activation/revocation each contributed
one revision. No provider request occurs. The first sandboxed invocation failed
because its temporary workspace registry was unavailable; the permitted native
invocation passed. Neither attempt is paid inference.

`scripts/evals/cs3-installed-skills-qualification.ps1` binds installation evidence
to offline discovery, explicit activation, canonical revocation, relocation, lazy
loading, integrity rejection and precedence tests against the installed executable
and catalog. `-CandidateRoot` selects the exact prospective six-candidate inventory
for the qualification tests; production discovery has no new environment override.
The runner captures candidate files before and after execution and rejects changes.
Candidate comparison, browser qualification and default promotion remain separate
gates.

The first exact-installed run passed all five tests: both six-candidate tests,
relocation/lazy-integrity, offline inspection without provider or budget admission,
and terminal activation/setup/precedence. Its accepted-candidate report is retained
at `artifacts/cs3-installed-skills/4baee7f1-c653-4d22-9064-1654ee1087ee/result.json`.
The simulated task responses are local test fixtures; paid requests remain zero.
The subsequent prospective run also passed all five tests and bound thirteen
descriptor/body/resource files before and after execution. Its six versions are
document-authoring 1.0.4, skill-authoring 1.0.2, frontend-design 1.0.0,
mcp-development 1.0.1, llm-integration 1.0.0 and webapp-testing 1.0.0 under
`src/evals/skills/cs3-comparison/candidates`. The report is retained at
`artifacts/cs3-installed-skills/9c71d35b-c344-4977-b1e1-345133334457/result.json`.
The native test source SHA-256 is
`f447d15a1a87be03f54f3640e926d51520405c6ca174153f4294c110454d514f`.
Any later candidate byte change requires fresh acceptance for that candidate.

After review added post-run executable, catalog and runner identity checks, the
same five tests passed again with all input identities unchanged and zero paid
requests. This final receipt is
`artifacts/cs3-installed-skills/82f1ed02-78db-44ef-b59b-8a7cde3f9d86/result.json`.
That receipt's runner SHA-256 is
`88d0898a94f8e27dc9c5c8b5e2fe2a93914c5fee49fe78c9527fc583c4f4a57f`.
Scoped Rust formatting and `git diff --check` also pass.

A subsequent build-output-only correction makes the runner pass an explicit
`--target-dir`, defaulting to `artifacts/codex-target`, and record the resolved
target root. This prevents generated Cargo output inside the frozen upstream
source tree. The earlier receipts bind the earlier runner, not this correction;
PowerShell parsing and diff validation pass for the corrected runner.

The generated default Cargo cache (created September 28 at 08:21 local time and
subsequently reused by qualification tests) was preserved by moving the entire
directory from `src/third_party/codex/codex-rs/target` to the new
`artifacts/cs3-preserved-default-target-20260928`. Absolute paths, non-reparse
ancestors and an absent destination were checked before the move. No files were
deleted or merged into another cache. Source-tree inventory must not traverse
generated build output. Historical executable locations now resolve under this
preserved directory; the package installation itself was not moved.

Before/after SHA-256 checks were identical for these preserved binaries:

| Cache-relative binary | SHA-256 |
|---|---|
| `debug/vcp.exe` | `25c339a79b584acb8766fadde27002ce242c6e9f363efea1bc2889fb7d83ee08` |
| `debug/vcp-provider-conformance.exe` | `eb7c6635a003c46f859f019c3fb5aa1833d6200af062aeb7fd62287c0cbd3225` |
| `debug/deps/executable-1791a9ff2bd59f75.exe` | `88691f589a5f9dfcf921bdd5b7cd87b2bbe371a6108ba309e7ddb5182dbb049c` |
| `debug/vcp-authoring-check.exe` | `c772a2bcc80b9a8429879177272f6108fa05b1f7c9accf681675ba7fb97d699b` |
| `debug/vcp-developer-check.exe` | `98f7453d660f68e564b1ff93d39bf8b01ae6e578e97902d84a656e11718716ce` |

## Explicit-target package refresh

The first explicit-target repeat is retained as a failed attempt at
`artifacts/cs3-installed-skills/9f9921da-5935-45e5-ace6-0112a442aef9/result.json`.
The fresh build succeeded, but both six-candidate tests rejected the old installed
executable because it differed from `CARGO_BIN_EXE_vcp`. No functional acceptance
is claimed for that attempt, and the exact-build guard was not weakened.

The subsequent package refresh keeps the authentic original main baseline archive
and upgrades to the rebuilt candidate. Production Rust and builtin assets remain
unchanged against `bd350386`; only the qualification test source differs under
`src/crates`. The archives now contain different executable bytes. Installation,
upgrade, exact baseline rollback and re-upgrade all pass, with the protected-data
sentinel unchanged at each step. Both use the same unchanged 21-skill catalog;
this still does not establish a catalog-version migration. The new manifest
truthfully retains `caller-supplied-unverified` build provenance, not a signed or
release-qualified build claim.

| Refreshed evidence | SHA-256 |
|---|---|
| Candidate archive | `21b76426969abcec46752a49de6fd1463ac7fa798da65817eb60464f3bff36bd` |
| Rebuilt candidate executable | `d08ff1069d6700a8aebc7ba668b510ce98867dc6ec2f68b7fed079b34bc5312e` |
| Four-stage installation report | `92680a29d33e3f53bc9cbc7bbd9f0eef42807cd43c5f6454b4b4728c9f614391` |
| Exact rebuilt installed-candidate tests | `37cf419c275beb5f4ea8dac97a9ef8d638db75dbb92c3ff5c639dab9e4cd58be` |

The refreshed package result is
`artifacts/cs3-package-current-results/a7a5798a-167f-423d-9221-2e5b2fac0ad6/result.json`.
The installation report is retained in the current user's temporary directory at
`vcp-authoring-install-a546aad7-206b-47a5-ae1c-41b2563f6caf/result.json`.

All five installed-skill tests pass for this refreshed candidate, including the
unchanged exact-build guard. The final receipt is
`artifacts/cs3-installed-skills/7ef3f8a8-4411-46cb-9a81-a095e9366410/result.json`;
it binds runner SHA-256
`c5aeef79a5139fdb51bc34b645e9dd72795c79b7a3f171c751563e8b5dd11ea7`,
the same native test source hash above, and the explicit target
`artifacts/codex-target`. Candidate files, installed executable, builtin catalog,
runner and test source identities remain unchanged before/after acceptance.
Paid requests remain zero. The frozen upstream source tree has no generated
`target` directory after the run.

The prospective `webapp-testing` wording was subsequently corrected to version
1.0.1: supplied qualified host observations do not grant the model browser
execution authority. Historical `src/skills/candidates` version 1.0.0 was not
changed. All five exact-installed tests pass again for this prospective inventory;
archive, executable, catalog, runner and native test source remain unchanged.
The final candidate-bound receipt is
`artifacts/cs3-installed-skills/f1431395-08ae-490d-b0b4-cc377b54ee4b/result.json`,
SHA-256 `55726d5fe6160c31eb58830bf86afee4526c44472602f541b6dd9fde33442e85`.
It records unchanged before/after candidate identities and zero paid requests.
The WEB 1.0.1 descriptor SHA-256 is
`ed0680c19a9719e2357412639e21e3a7dcf162efb5c1c3c225d8209489fc5a8f`;
its body SHA-256 is
`12cebf4451700950a032d1923a5cd39b75b21a30728973bdf903d799e4d71e4f`.

The separate [retained frontend recovery record](cs3-retained-ui-recovery.md)
describes the still-missing historical UI output evidence.
