# P8 request-allowance context follow-up

Status: implemented; four focused native tests passed. This is a
P8-05 readiness correction within P6-03 bounded orchestration and P2-08 current
context assembly. It does not complete owner-task or human acceptance.

## Observation and scope

The approved September 23 owner campaign's U01 SQLite and files slots each
stopped at the configured 16-request limit. Both retained 16 completed provider
responses with tool calls and no final answer. SQLite performed 18 successful
read/list operations; files performed 17. Their final ledgers had no active or
unresolved liability. Both paused well before the 900-second deadline with
budget remaining.

Read-only inspection of all 32 captured request bodies found no model-visible
request limit or remaining-request observation and no batching guidance in the
operating instructions. Every request advertised scoped `vcp_search`.
Multiple calls already worked in a single response; most responses nevertheless
requested one read or directory listing. These observations identify missing
work-budget context. They do not establish that it caused either failure, that
16 requests cannot suffice, or that changing the prompt will complete a task.

The [paid campaign](p8-approved-campaign-2026-09-23.md) evaluated its immutable
executable and inputs. This Rust
source correction did not change that campaign or authorize another paid trial.
Native compilation and tests started only after all approved paid executions
were terminal.

## Implemented contract

Each coding-context assembly captures a fresh `canonical_root_request_allowance`
observation as an observed task-state part, through the existing artifact and
context-manifest path. It reports the canonical root, the minimum request limit
of registered coding loops, the count of canonical attempts belonging to that
root, and `requests_remaining_including_this_request`.

The observation is taken before admission of the request receiving that context.
Consequently the remaining count includes that request. Children, helpers and
retries consume the same root allowance. Attempts are not refunded according to
their outcome; unrelated roots do not contribute. A concurrent admission may
consume allowance after the observation. The observation is neither a reservation
nor permission to dispatch.

The admission gate and observation use the same calculation. The existing
deadline validation and request-limit refusal remain authoritative. No request
cap, permission, automatic completion rule or retry policy changes.

Static operating guidance recommends batching independent known read/list/search
calls, using bounded cross-file search, and planning an opportunity for the final
answer after required checks. Dependent operations must retain their ordering;
verification and MCP calls retain their isolated-response requirement. Insufficient
evidence must be reported honestly, without invented results or skipped checks.

## Validation

Formatting and diff checks passed. Native Rust 1.95.0 compiled and ran the
following exact tests using `--locked --offline -p vcp-lifecycle -j2`, the warm
target directory, the existing VS/native configuration, and a 16 MiB Rust test
thread stack. Each exact invocation reported one passed, zero failed and zero
ignored. Existing dependency/private-interface warnings remain unrelated.
Independent source review found no blocking correctness or security findings.

- `foundation::worker::coding::request_allowance::tests::allowance_uses_minimum_shared_limit_and_counts_root_attempts_without_refunds`:
  independent count/minimum arithmetic, unrelated-root exclusion, exhaustion,
  over-limit saturation and absent coding configuration.
- `coding::coding_context_lists_only_public_process_invocation_metadata`:
  actual loopback request allowance and captured provenance on both stores,
  alongside existing public-metadata and secret-boundary assertions.
- `coding::canonical_coding_loop_assembles_current_sources_and_dispatches_prepared_files`:
  fresh observations across actual HTTP turns, canonical capture hashes,
  retained count after reopen, and the existing refusal when a helper's larger
  local setting would exceed the root cap.
- `provider_retries::provider_retry_reassembles_coding_continuity_with_current_liability`:
  the retry request includes its predecessor in used allowance while retaining
  the predecessor's unresolved liability.

The unit test used `--lib`; the other three used `--test canonical_host`.
Every invocation ended with `-- --exact --nocapture`. Retained local stdout/stderr
receipts are under `artifacts/`:

| Receipt | Test time | SHA-256 |
| --- | --- | --- |
| `request-allowance-unit.log` | 0.00 s | `e9b8aa69200df91104d4cb4321b0389078f2c123c99d4e9fe3d248f6087c4f57` |
| `request-allowance-metadata.log` | 2.51 s | `b4eb90e713cc335ff3611ec878fc4878a1e5930df2fba3fcea164ee78f3eb7c9` |
| `request-allowance-loop.log` | 72.00 s | `89eb54f28c58e6072be0abfd671b697e29817b0f4293154bc1c8005b29159db8` |
| `request-allowance-retry.log` | 2.80 s | `648f29f12e6815766d161cee07a41f3021ebe67f2f441d3e7f330e490093b809` |

The shared-root arithmetic and existing helper refusal do not independently
exercise an admitted child HTTP request followed by a parent allowance
observation. The tests also do not measure model completion efficacy. Any later
paid evaluation requires its own authorized, frozen inputs and honest comparison;
passing these checks alone does not qualify owner-task completion.
