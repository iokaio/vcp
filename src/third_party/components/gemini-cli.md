# Gemini CLI fixture and port candidates

Origin: `https://github.com/google-gemini/gemini-cli`, immutable revision
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`. State: pinned reference inputs plus
the bounded attributed Rust port recorded below. Owner: P0-07 selection and P0-09 boundary
fixtures/ports. Root LICENSE and the selected source headers declare Apache-2.0;
the inspected TypeScript headers identify Copyright 2025 or 2026 Google LLC. Preserve
those terms and record attributed ports when source is actually included.

| Responsibility | Concrete candidate input | VCP requirement |
|---|---|---|
| G01 tool invocation | `packages/core/src/tools/tools.ts` and `tools.test.ts`; `ToolInvocation`, `BaseToolInvocation`, `DeclarativeTool`, `ToolResult` | Neutral canonical arguments, prepared effects, current permission ceiling and receipts |
| G02 scheduling | `packages/core/src/scheduler/scheduler.ts`, `scheduler.test.ts`, `policy.ts` | Root-owned scheduling; cancellation, conflicts and out-of-order results must not bypass VCP authority/budget |
| G03 policy | `packages/core/src/policy/policy-engine.ts`, `policy-engine.test.ts`, `types.ts`, `stable-stringify.ts` | Preserve compatible decision behavior with explicit argument hashes; model text never grants authority |
| G06 skills/MCP | `packages/core/src/skills/skillLoader.ts`, `skillManager.ts`, `tools/mcp-client.ts`, `mcp-tool.ts` | Candidate seams only; VCP trust, endpoint/auth and cancellation contracts still govern |

The [native boundary report](../../../docs/evaluations/p0-07-gemini-baseline.md)
records 402 passing policy/scheduler/tool/skill/MCP tests with Node 24.10.0,
TypeScript 5.8.3 and Vitest 3.2.4. These are mocked upstream unit tests, not
evidence of a VCP port, live provider control or operating-system enforcement.
The [reproduction and effect guide](../../../docs/development/gemini-baseline.md)
records the fixed suite list and intentional VCP adaptation requirements.

Root `package.json`, `package-lock.json`, `packages/core/package.json`, the core
test configuration and `scripts/generate-git-commit-info.js` are required baseline
setup inputs. The first selected run failed because generated Git metadata was
missing; invoking that inspected generator and rerunning passed. Expanded MCP
tests also need the core TypeScript build; the runner records both stages. Keep acquisition,
dependency installation and this explicit preparation separate from VCP builds.

The candidate files are not a closed Node production extraction. Imports include
the Google SDK types, shell parsers, policy/checker/sandbox interfaces and logging
helpers; scheduler/tool implementations can trigger filesystem/process/network
effects through those collaborators. P0-09 supplies the language-neutral
fixtures, exact semantic comparisons and bounded attributed Rust
adapters recorded below. Preserve provider-independent behavior while
excluding SDK-specific types and implicit provider/telemetry routes. Any retained
code needs its complete selected dependency/license closure at import time.

Hooks and editor utilities remain deferred research. Node is comparison tooling;
this candidate does not add a second production engine or relax the OpenRouter,
local-embedding, one-store or one-ledger requirements.

## P0-09 attributed adaptation

The [integration guide](../../../docs/development/p0-integration.md) and
[qualification report](../../../docs/evaluations/p0-08-09-integration.md) record
the actual comparison. `packages/core/src/policy/stable-stringify.ts`,
`scheduler/state-manager.ts`, `scheduler/types.ts`, `scheduler/policy.ts` and
`policy/policy-engine.ts` are the behavioral source inputs. Their inspected
headers declare Copyright 2025 or 2026 Google LLC and Apache-2.0. The original
[license text](../licenses/gemini-cli-6a466a7e-LICENSE) is retained with SHA-256
`58d1e17ffe5109a7ae296caafcadfdbe6a7d176f0bc4ab01e12a689b0499d8bd`.

Destinations are `src/crates/vcp-lifecycle/src/ports.rs`, its `tests/ports.rs`,
`src/tests/fixtures/gemini/ports.json` and
`scripts/upstream/compare-gemini-ports.cjs`. The Rust adapter and scenarios are
attributed adaptations, not independent authorship. No Google SDK, Node package
or TypeScript runtime source is copied into the application. The Rust closure is
the existing `vcp-lifecycle`/Serde/SHA-256 lockfile closure. The pinned Node lock
continues to govern only the explicitly acquired comparison checkout.

The adaptation preserves canonical structural delimiters, call identity,
argument replacement and observed result ordering. It narrows the value domain
to a declared JSON-compatible subset and adds revision-bound approvals, trusted
ceilings, durable-intent prerequisites, resource conflicts and partial-effect
receipts. The fixture records original outcomes alongside intentional VCP
differences. The comparison manifest binds exact TypeScript/compiled module and
fixture hashes. No generated binding or provider-specific type crosses the Rust seam.
