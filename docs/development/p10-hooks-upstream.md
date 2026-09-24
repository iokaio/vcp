# P10-01 Gemini G04 qualification and deliberate differences

G04 uses the existing immutable Gemini candidate,
`6a466a7e2fe2b1255752c1e74f69b31f0216084d` (tree
`5cb35fa93b203b709e91737b0a00da985dbb186e`). The original P0-07
402-test baseline did **not** qualify hooks. P10-01 adds a separate upstream
fixture selection at [hooks-upstream.json](../../src/tests/fixtures/gemini/hooks-upstream.json).
It retains upstream expectations independently of VCP's hook tests.

Acquire and prepare the candidate using the existing
[baseline guide](gemini-baseline.md), including lockfile installation with lifecycle
scripts disabled. Execute from the repository root on native Windows:

```powershell
node scripts/upstream/test-gemini-hooks.cjs --source artifacts/upstream/gemini-candidate --output-root artifacts/gemini-hooks-qualification --prepare
node --test src/tests/contracts/gemini-baseline.test.cjs
```

The wrapper uses the existing isolated delivery harness: clean exact commit/tree
before and after execution, synthetic home, filtered environment, ten-minute stage
limits, 16 MiB logs and process-tree cancellation. It executes only the inspected
metadata generator, core TypeScript compiler and three pinned test files. The
manifest binds source identity, VCP dirty state, compiler/test versions, lockfile,
runner, wrapper, fixture, policy and result hashes. Any missing, skipped, duplicate
or failed assertion fails qualification. A separate profile leaves the 402-test
P0-07 requirement intact.

| Pinned test file under `packages/core` | Assertions | Observed boundary |
|---|---:|---|
| `src/hooks/hookRegistry.test.ts` | 24 | Configuration validation, trusted project loading, enabled extensions and named enable/disable |
| `src/hooks/hookPlanner.test.ts` | 10 | Event selection, regex and exact-trigger matching, first-entry name/command deduplication |
| `src/hooks/hookRunner.test.ts` | 25 | Sequential/parallel callbacks, timeout, failure continuation, input chaining and permissive output parsing |

The tests mock process spawning and collaborators. They qualify the selected
upstream behavior, not operating-system containment, VCP authorization or crash
recovery. VCP's broker and durable lifecycle tests must establish those separately.

## Adaptation contract under ADR-014

The inspected production inputs are `hookRegistry.ts`, `hookPlanner.ts`,
`hookRunner.ts` and `types.ts` in the same directory. Their headers identify
Copyright 2025 Google LLC, Apache-2.0. The existing
[retained license](../../src/third_party/licenses/gemini-cli-6a466a7e-LICENSE)
applies to attributed adaptations. No TypeScript production source, SDK types,
Node runtime or upstream settings loader is imported into VCP.

| Upstream behavior observed in source or selected fixtures | Deliberate VCP requirement |
|---|---|
| Registry sorts Runtime, Project, User, System, Extensions; ties retain registration order | Explicit configured priority and stable identity determine order; duplicate identities and ambiguous/cyclic constraints fail validation |
| Planner deduplicates name/command and can run all hooks in parallel unless any entry requests sequential execution | Pure deterministic plan, bounded fan-out, version/source/input identity and durable delivery deduplication |
| Trusted project hooks are warned about and remembered; invalid config is skipped | Explicit hook admission and granted scope; malformed/security-relevant configuration blocks the affected action |
| Runner builds sanitized ambient environment plus hook overrides, expands shell variables and starts a shell | Existing VCP broker owns effects; explicit environment, artifact and effect scope; no inherited credentials or implicit shell authority |
| Runner collects stdout/stderr as strings without a byte cap | Bounded output, timeout and strict versioned result validation before publication |
| Malformed JSON becomes plain text; exit 0/1 can produce `allow`; double-encoded JSON is accepted | Malformed/security output blocks; optional notification failure is visible; a timeout cannot grant authorization |
| Sequential success can shallow-merge `tool_input` for the next hook; execution continues after failures | Rewrites change operation identity, invalidate old approval and return through schema/resource/policy checks |
| Runner has no durable intent/effect journal or trigger-depth contract | Shared durable intents, recursion bounds, pause fencing, stale-result validation and reconciliation of uncertain effects without automatic replay |

This is a qualified behavioral adaptation, not Gemini configuration or hook-wire
compatibility. P10-02 remains responsible for any explicit imported field subset.
Executable hooks remain separate from first-release skill discovery.
