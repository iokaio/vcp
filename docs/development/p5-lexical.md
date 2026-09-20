# P5-03 lexical qualification

Status: actual Tantivy quality fixture passed on native Windows, 2026-09-19.

The VCP-owned Tantivy adapter indexes canonical IDs, exact workspace/task/root,
path/symbol, kind/status, source version/digest, sequence and byte span fields.
Only IDs, ranks and scores leave the adapter. Canonical current authorization,
retention and source validation remain required before dereferencing a candidate.
Lexical components are immutable private directories; this increment does not
claim P5-05 activation or any ANN readiness.

The schema uses separate prose and code tokens, no stemming or stop words,
exact-match boost 8 and code-term boost 2. These settings and top-k 3 are declared
before running the fixed query fixture. Exact filters run inside the query before
rank truncation. Do not tune these settings against the reported query results.

## Reproduction

The harness at `src/crates/vcp-memory/examples/lexical_quality.rs` embeds the unchanged fixture at
`src/tests/fixtures/local-memory/corpus.json` and needs no model assets or network.

From `src/third_party/codex/codex-rs`, using the repository's native Rust/MSVC
environment and shared target directory:

```powershell
$env:CARGO_TARGET_DIR = 'D:/code/Github/vcp/artifacts/codex-target'
cargo +stable run --locked --offline -p vcp-memory --example lexical_quality -j4
```

The JSON report records the fixture digest, schema/tokenizer versions, weights,
build times, per-query candidate and source IDs, reopening/validation times,
first-query times and five warm-query repetitions. Record the command, source
commit, Windows version, Rust version and debug/release mode with retained output.
The retained report is `artifacts/p5-03-quality.json`.

The measured build used Rust 1.98.1 in the debug profile, Windows 10.0.26200 x64,
an AMD Threadripper PRO 5975WX (64 logical processors) and 137,295,024,128 bytes
of RAM. The fixture SHA-256 is
`f5bc7532611892da77cc4e9ab793054d01ac2475e316ba530895c9d0ea08337e`.
Atlas indexed 11 current records and excluded one superseded version; Boreal
indexed 12. Builds took 71.853 ms and 67.017 ms respectively.

Both lexical-labelled queries retrieved their one required source on first and
warm reads: macro recall@3 1.0, precision among returned records 1.0, and
precision@3 0.3333 (the two unused result slots count in the denominator).
All seven queries, including the five semantic-only diagnostic queries, had
fresh-reader query p50/p95 of 649/717 microseconds. Across 35 warm queries the
values were 545/663 microseconds. Separate reopen and full component validation
p50/p95 were 5.107/5.278 ms. Percentiles use nearest rank. These observations are
lexical-only; no vector or fused quality is inferred.

## Metric definitions and acceptance

The fixture contains 24 source versions across Atlas and Boreal. The obsolete
Atlas pause version is explicitly excluded from the authorized fixture inventory;
its replacement remains searchable. The two workspaces contain conflicting
`pause_session` meanings, preventing a single unscoped namespace from passing.
Each document gets its own task, a workspace root, deterministic fixture path,
function symbol and immutable artifact-source identity.

Seven existing queries run without expected-answer scope hints. Two have
`lexical_required` labels; their precision and recall are reported separately
from the five queries carrying only semantic labels. The latter are diagnostic
observations of this lexical adapter, never reported as vector or fused results.

- Precision@3 is relevant returned source IDs divided by three, including empty
  result slots. Precision among returned results uses the actual returned count.
- Recall@3 is relevant returned source IDs divided by the declared relevant set.
- Macro metrics average only rows with lexical-required labels. Fresh-reader and
  warm-reader summaries remain separate; repeated warm runs do not add queries.
- The harness fails if any lexical-required source is missing, results cross a
  workspace or expose a superseded source, or warm source-ID ordering changes.
- Separate scope checks restrict task/root/path/symbol before a one-result limit,
  verify that an empty task scope returns nothing, and reject another workspace.
  These checks are excluded from the quality score.

This is the repository's published held-out fixture, not a new blind corpus. Two
lexical-labelled queries provide limited recall evidence. Fresh-reader means a
new reader after component reopening and validation; filesystem/OS caches are
not flushed, so physical cold-cache latency is not claimed. Build and query times
must not be presented as production-scale performance guarantees.

## Component boundary evidence

The companion lexical adapter tests cover exact case and Unicode paths, qualified
names, short identifiers, prose phrase order, filtering before result limits,
IDs-only output, explicit merge followed by reopen, replacement inventories that
remove old versions, and old-reader reuse while a new component exists. Resource
and failure cases include source bounds, nonempty output-directory refusal,
empty inventories, cancellation before and during private construction, modified
metadata, incompatible tokenizer manifests and Windows junction redirection.
All 52 memory tests passed on stable Rust 1.98.1, including six lexical adapter
tests and five chunk/inventory tests. Both storage backends cover retained
source snapshots, exact encoding/spans, corrected claims, rebind exclusions and
hidden-origin access after pruning. Clippy for all memory targets passed without
warnings in the changed crate. Exact patch replay, source inventory and boundary
checks passed; Tantivy remains the existing 0.22.1 pin.
The production CLI build and all eight fast delivery checks also passed.

Reproduction:

```text
cargo +stable test --locked --offline -j4 -p vcp-memory --tests
cargo +stable clippy --locked --offline -j4 -p vcp-memory --all-targets --no-deps
cargo +stable build --locked --offline -j4 -p vcp-cli --bin vcp
scripts/test.ps1 -Suite fast
```

Native logs are `artifacts/p5-03-tests.log`, `p5-03-clippy.log`,
`p5-03-build.log` and `p5-03-fast.log`.

P5-05 owns publication and generation consistency. Passing these tests does not
authorize an obsolete pinned reader to bypass today's retention or access checks.
