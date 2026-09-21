# P6-02 / M1 — Consumed-value retention and replay

Status: implemented and locally verified, September 20, 2026. This is the eighth
and final M1 foundation increment. It qualifies no statistical value and enables
no routing consumer.

## Behavior

The [selected boundary](../adr/035-consumed-statistical-value-replay.md) persists
only an exact reward scalar selected for local optimization inspection. A fresh
authorized producer rebuild must match before the first write. The receipt binds
the consumer decision, producer and source identities, selection, source-attempt
digest, task scope, policy/catalog and exact currency/micros value. Canonical
references identify the selected tasks, attempts and settlements.

Idempotent reuse returns the original receipt only for identical inputs. A stale
artifact, conflicting decision reuse, abstention or unknown cohort cannot create a
receipt. Replay validates receipt integrity and task access, then uses the recorded
value without refitting. The receipt is historical-only and cannot serve routing.

## Verification record

Files and SQLite fixtures consume a complete two-attempt cohort, check exact
canonical references and digests, then add an uncertain attempt that changes the
current artifact. Replay still returns the recorded value; a new decision rejects
the stale artifact and the current unknown cohort. The fixtures also cover
idempotency, task-scope denial and reopen parity.

Native Windows MSVC verification used installed stable Rust:

- `cargo +stable test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **20 passed**, including consumption/replay on Files and SQLite.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `6063e1ad-fea8-431f-8aa6-74f7f4c137d9`.
- Stable changed-file `rustfmt --check`, repository/link contracts and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No paid, live-provider or
packaged qualification was run.

## Remaining

M1 is complete. The statistical values remain unqualified. M2 is the next planned
increment under P6-03; M3/M4 remain responsible for forecasts, frozen
qualification and any enabled consumer.
