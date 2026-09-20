# P3-03 evidence inspectors

P3-03 implements the common navigation/query contract and existing-state views.
Memory search/evidence is documented in [P5-06 retrieval](p5-retrieval.md);
retention selection and deletion UI remains
P3-05/P5-07. Grouped routing is not advertised as available.

## Implemented contract

`vcp-audit::inspection` owns `InspectionQuery`, `InspectionPage`, view filtering,
canonical-key/event-watermark ordering, scope checks, cursor validation and
artifact ranges. CLI adapters issue the same query against a closed store or the
current owner's serialized worker. They cannot dispatch effects. Current access
and retention are enforced before artifact I/O; a known ID grants no access.

`chain` shows typed record references and canonical events for task, captured
request/context, attempt/reservation, prepared tool/policy, dispatch/outcome and
verification. Existing artifact schemas identify manifests, tool preparation,
process/file receipts and raw provider observations. Content loads only on
request. Requested model identity and served-identity availability are separate;
raw response evidence remains the source for observed served identities and usage.

Pages carry a source watermark and reject continuation after canonical changes.
They contain at most 128 items/512 KiB of record/event payload. Artifact reads
retain at most 64 KiB of content plus spool/serialization overhead, preserve arbitrary bytes, and explicitly
report pruned, missing, incomplete and excluded content. Valid UTF-8 is also
returned as JSON-escaped display text. Oversized records/events carry a truncation
marker. The immutable spool's full-digest verification still scans the artifact
for each range; memory bounds do not claim constant-time or bounded-total-I/O
random access.

See [CLI examples and wire shape](p3-cli-usage.md). The inspector result payload
replaces P3-01's provisional capped record list; outer JSONL version 1 is unchanged.

## Qualification

Native Windows, stable Rust, offline dependencies, Visual C++ x64 tool environment.
Package tests exercise both canonical backends, paged relationship traversal,
projection rebuild parity at the same watermark, cursor/filter/access changes,
retention masking, binary ranges, omitted capture, missing content and corruption.
The executable fixture independently parses JSONL and compares inspected request
bytes with bytes received by its synthetic HTTP server, both with a closed store
and through the running owner's private pipe. It also retains verification,
control, resume, fork, budget and output-loss regression coverage.

Completed on 2026-09-19. Commands run from `src/third_party/codex/codex-rs`
with `CARGO_TARGET_DIR=artifacts/codex-target` (absolute repository path):

```text
cargo +stable test --locked --offline -j 4 -p vcp-audit -p vcp-cli --features vcp-audit/qualification,vcp-cli/qualification --tests
cargo +stable test --locked --offline -j 4 -p vcp-context --tests
cargo +stable test --locked --offline -j 4 -p vcp-lifecycle --test canonical_host fresh_process_history_preserves_actual_unknown_process_paused_child_and_late_charge
cargo +stable clippy --locked --offline -j 4 -p vcp-audit -p vcp-cli --features vcp-audit/qualification,vcp-cli/qualification --tests --no-deps
```

- Audit: 8 passed, including the Files/SQLite and fresh-process cases.
- CLI: 20 passed, including all 4 executable cases (73.27 seconds).
- Context: 13 passed. Retained fresh-process recovery: 1 passed.
- Clippy: passed without local lint warnings; the existing dependency
  `proc-macro-error2` reports a future-Rust compatibility notice.
- Changed Rust files pass `rustfmt +stable --edition 2021 --config skip_children=true --check`.
- All 8 registered `fast` checks pass when invoked directly with their exact
  `src/tests/registry.json` arguments. The aggregate harness could not hash source
  under this sandbox's different repository owner: its sanitized child environment
  removes the command-scoped Git safe-directory override. No global Git settings
  or harness trust rules were changed. The repository check was rerun after fixing
  the ledger status spelling; it reports 180 Markdown files, 1,679 relative links,
  68 tasks and no errors.
- Selected Codex source inventory: 7,939 files verified; dependency boundaries:
  173 packages, 36 groups, 86 seams. Patch 0023 records only the CLI's dependency
  on the existing local audit package; no external dependency version changed.
  Reverse patch applicability and `git diff --check` pass.

Local ignored logs are `artifacts/p3-03-final-tests.log`, `p3-03-context.log`,
`p3-03-recovery.log`, `p3-03-clippy.log` and `p3-03-fast-*.log`. The HTTP fixture is
synthetic; this is not a paid-provider compatibility or interactive-console claim.
There are no remaining P3-03 common/current-state acceptance conditions. Future
memory/retention features retain their own gates and use this shared contract.
