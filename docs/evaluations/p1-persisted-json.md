# P1-04 persisted JSON qualification

Status: native storage maintenance qualification passed.

The [decode contract](../development/p1-persisted-json.md) repairs literal object
preservation during canonical replay without changing serialization or hashes.

## Reproduction and isolated evidence

The actual production CLI feature tree already includes serde_json
`arbitrary_precision` and `raw_value`. An isolated program using the actual
unmodified Store/Record/Event/Transaction implementations committed twelve cases
across Files and SQLite: each private marker in a record, event or both. All
twelve failed reopen with a durable receipt mismatch. The baseline is retained in
`artifacts/p7-mcp-numeric-store-probe/results.json`.

The isolated repair reopened all twelve original failing stores with identical
State values and canonical bytes, using their original receipts. It also passed
eighteen fresh store cases including exact long fractions, integers above u64 and
exponents. Evidence is in `artifacts/p7-mcp-numeric-store-fix/reopen-original.json`
and `results-numeric.json`. These are preparation evidence, not a replacement
for the tracked native campaign.

Review found that field adapters alone break internally tagged enum decoding.
The prototype therefore also covers actual Mutation and EventPage decoding,
including owned and borrowed Value conversions. Earlier failed prototype logs
remain retained. The tracked tests must also exercise checkpoint, replay-base,
portable snapshot and backend conversion paths and existing history consumers.

## Native campaign

The frozen campaign passed **182 tests** across ten stages on Windows 10.0.26200,
Rust 1.98.1 and MSVC 14.44.35207. Source identity remained unchanged throughout.

| Stage | Passed |
| --- | ---: |
| Protocol, default feature graph | 7 |
| Protocol, arbitrary-precision feature graph | 7 |
| New store integration cases, default graph | 5 |
| Full store regression, exact-number graph | 62 |
| Lifecycle boundaries | 50 |
| CLI | 47 |
| Fresh-process history, retention, backup and MCP coding consumers | 4 |

Three existing environment-dependent tests remained ignored: the store and CLI
actual-cloud-directory cases, and the independent age/Node envelope verifier.
They were not run or counted as passing by this campaign.

Manifest:
`artifacts/p1-persisted-json-qualification/e4ef5a9d-deb8-40dc-ac7a-ec81438a5d28/manifest.json`.
Source content:
`a89b0db36af646b012476068587ab3375871029f9e09e69420991bb25fbe90e3`.

The new store cases cover actual Files and SQLite replay, original idempotent
receipts after reopen, Files checkpoints, replay-base rewrite plus subsequent
commits, both backend conversion directions and portable history capture/decode/
staging. Literal marker objects, escaped keys, siblings, both markers, malformed
marker-looking strings and ordinary numeric values retain their canonical bytes.
Typed Record, Event, Mutation, Commit and State conversions are checked directly;
protocol tests also cover EventPage and its additive-field behavior.

Depth tests compare the new parser against every old-reader success across a
0–132-level fixture range, then reject excess depth, byte length and decoded
duplicate keys. Serialization remains covered by independent fixed canonical
literal-object bytes. No integrity comparison was weakened.

All-target Clippy for protocol and store passed in
`artifacts/p1-json-clippy.log`; existing warnings in unchanged code remain visible.
Production CLI checking passed in `artifacts/p1-json-production-check.log`.
Affected Rust formatting and `git diff --check` passed.

All nine fast repository delivery gates passed, including documentation,
dependency/effect inventory and selected-source reconstruction:
`artifacts/p1-json-fast/dceaf0dd-ecd0-4a33-867c-8bf1376ba071/manifest.json`.
