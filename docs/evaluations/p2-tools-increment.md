# P2 prepared native file increment

P2-03 and P2-04 remain `in_progress`. This increment supplies immutable file
preparation and canonical dispatch, building on the
[authority core](p2-policy-increment.md). Process/PTY policy integration and
retained model-tool scheduling remain required before task completion. See the
[implementation and reproduction guide](../development/p2-tools.md).

## Qualified behavior

| Boundary | Observed acceptance |
|---|---|
| Preparation | Read, list, literal search and multi-file patches perform no candidate writes; schemas reject extra authority fields; full source versions and exact arguments bind the operation |
| Patch semantics | Retained Codex parsing with exact unambiguous matching; CRLF, UTF-8 BOM and BOM-marked UTF-16 preservation; overlapping changes and unsafe paths reject |
| Native mutation | Owned handles preserve source identity; stale content, conflicting handles, hard links, junction escapes and newly occupied destinations reject; creation, update, rename, case-only rename and deletion execute on Windows |
| Native names | Long paths and 16 filename-length alignments pass; the final native handle path is checked before success |
| Canonical broker | Both canonical stores reject stale source/policy tickets; plan mode denies writes; questions wait durably; answering does not resume; deliberate resume permits the exact grant |
| Receipts and partial effects | Each file has captured intent/outcome; an incompatible handle on a later file retains the earlier applied change and an incomplete outcome; no rollback or replay |
| Repository boundary | The real Git index remains byte-identical through broker edits; no provider requests occur in these fixtures |
| Bounded observations | Read/list/search ceilings and changed source observations reject or explicitly report incomplete results |

Native testing exposed an incorrectly sized variable-length rename buffer. The
fix retains the byte length, adds an explicit UTF-16 terminator and checks the
actual post-mutation handle path. The alignment regression covers the original
failure. An initial integration-runner assertion still expected six canonical
tests; it was updated to require all seven after the new broker test passed.

## Source and environment

Local qualification uses Windows 10.0.26200, NTFS, MSVC 14.50.35717 and the explicit
Rust 1.98.0 experiment. The selected upstream Rust 1.95 pin is unchanged.
Tests use synthetic local workspaces and loopback providers, without paid calls.

Patch `0017-p2-prepared-files.patch` adds a pure entry point to the retained
Codex patch machinery and registers the original VCP crate. External dependency
pins are unchanged. Independent reconstruction matches all 7,938 selected files:

- Patch SHA-256: `2586f5646042619a7a54137814f25f8fcc71b976119107105150ec6cc3c33048`.
- Reconstructed aggregate: `f08100ac0284dc4cc1dc2273037b0178181729caa483ceadfbe1c3bbe62fd6ec`.
- Boundary inventory: 172 packages, 35 groups and 77 classified seams.

## Local evidence

All commands below ran from the repository root with exit status zero. Native
manifests record the base commit and the hashes of the uncommitted source under
test, alongside stage commands, versions and log hashes. Raw evidence remains
under ignored `artifacts/`; it is not a clean-clone prerequisite.

| Command | Result | Local manifest |
|---|---|---|
| `pwsh -NoProfile -File scripts/test-tools.ps1` | 26 contracts and 65 retained patch tests passed | `artifacts/tools/ad74d7a0-9ef5-497c-a14d-5ac208b9800e/manifest.json` |
| `pwsh -NoProfile -File scripts/test-integration.ps1` | 34 host contracts, two containment regressions and seven-request CLI trace passed | `artifacts/integration/c78b04d2-61bf-4f77-bbf2-d4c5089769de/manifest.json` |
| Native `RecoveryTests` command in the guide | Process/filesystem/network recovery qualification passed | `artifacts/build/eec99a3d-c52f-490e-84da-9e4a264fe0e0/manifest.json` |
| Native `LifecycleTests` command in the guide | All 29 retained lifecycle regressions passed | `artifacts/build/22911e05-a27e-418d-a1d3-81608766a509/manifest.json` |
| `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` | All eight delivery/source cases passed | `artifacts/tests/b24c8e9b-f4b8-47e3-937d-a31610bf2175/manifest.json` |

Manifest SHA-256 values:

- Tools: `69d5565c6b4b844cfcfb3bf35f79cca6b69cc528fd1ef87cbaf22d4970c85b16`.
- Integration: `c4ec670649759b7547a91a9626827eea6107986351c846fe148194e0945ab35f`.
- Recovery: `8b79dfc069959898bda600475e1f5a8558297d8c4ea52ee538a7dbef05cb14ce`.
- Lifecycle: `8a40d494d707782a25eb97355008ce2c2081e51cde2ab6266b6f33e6b9119647`.

These local results are separate from the PR's remote CI confirmation.

## Remaining acceptance

Staging and flushing an existing file is not a crash-atomic replacement: an I/O
failure or crash can leave partial bytes. Captured intent and observations must
be reconciled. Memory-mapped writers and hardware power-loss durability are
outside the measured sharing/flush envelope. These primitives do not establish
a general filesystem or network sandbox.

Process/PTY profiles, executable and environment authority, automatic instruction
and context fences, retained tool scheduling, full verification, console-close
recovery and accounted compaction remain P2 work. This report does not qualify an
installed product or live OpenRouter compatibility.
