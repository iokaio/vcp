# P2 repository and context foundation

P2-01 is in progress. `vcp-repository` and `vcp-context` are original packages in
the retained Cargo workspace. They supply native observations and reproducible
context selection. The [OpenRouter increment](p2-provider.md) connects these seals
to actual retained requests; automatic turn assembly remains P2-05.
See the [increment qualification](../evaluations/p2-context-increment.md) and
[owning plan](../plan/04-context-and-instructions.md).

## Native observations

`vcp-repository/src/path.rs` registers explicit workspace/root/repository/worktree
identity and binding revision. Windows handles open each ancestor without
following reparse points, retain directory identity, and deny replacement while
reading. File handles deny concurrent writes/deletion. Selected versions include
native file identity, content hash and byte length. Non-Windows calls report an
unsupported capability rather than claiming equivalent enforcement.

`discovery.rs` performs bounded traversal using the existing `ignore` parser for
scoped `.gitignore` rules. It records ignored, generated, binary, oversize,
reparse and capacity exclusions. Present and absent ignore files are dependencies.
`observation.rs` hashes a versioned manifest including untracked content, rather
than treating HEAD or Git status alone as a content fingerprint.

`git.rs` observes an ordinary Git directory using an explicitly supplied native
executable and environment. It reuses the retained Windows Job Object, bounds
time/output, disables optional index writes and executable diff/filter routes,
and compares status/index/HEAD/diffs again before returning. It currently rejects
linked worktrees, alternate object roots, configuration includes, filters and
extension metadata. Those need explicit metadata scopes and additional native
qualification. No Git command grants tool authority.

`instructions.rs` is the single proposed VCP AGENTS.md loader. An explicit parent
root permits only its instruction read through this API; it does not expand the
workspace. Documents retain applicability for each affected path. Missing nested
instruction files are probes so their later creation invalidates prepared context.
Foreign instruction formats are not loaded. The explicit OpenRouter transport
profile disables the retained upstream loader so captured VCP instructions own
the request. The ordinary upstream CLI retains its own behavior.

## Selection and sealing

`vcp-context/src/manifest.rs` represents logical trust separately from content,
records source artifacts/ranges/versions, and seals serialized bytes with scope,
steering, authority, deletion, binding, instruction, schema, skill, memory,
task-state and model-envelope revisions. The host supplies a qualified encoder
and counter. The included UTF-8 byte ceiling is explicitly estimated; it is not
a provider tokenizer qualification.

`selection.rs` keeps mandatory instructions/objective/current state and complete
tool pairs together, ranks optional evidence deterministically, deduplicates
overlapping source ranges and records exact exclusions. An envelope too small
for mandatory content fails. Unsupported tool/role semantics fail rather than
flattening logical trust. Canonical artifact resolution compares every selected
range with captured bytes before returning `VerifiedContext`. This proof grants
no budget, transport or tool authority. The host must still revalidate revisions
and sources at send admission and enforce current artifact access.

An optional repository map is not selected in this increment. Bounded path/Git
discovery is the baseline; any future map requires a pinned comparison of useful
evidence per token. No map implementation or comparative result is claimed.

## Reproduction

```powershell
pwsh -NoProfile -File scripts/test-context.ps1
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The context runner requires native Windows, Rust 1.98.0, Visual C++ x64 tools,
Git and the existing offline dependency cache. It verifies selected source and
records input hashes, commands, toolchain, filesystem and the 19 actual tests.
Missing prerequisites return `not_run`. Ordinary baseline toolchain selection is
unchanged. Synthetic fixtures use disposable directories; no credentials or
paid calls are required. Outputs remain under ignored `artifacts/context/`.
