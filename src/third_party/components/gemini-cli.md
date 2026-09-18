# Gemini CLI fixture and port candidates

Origin: `https://github.com/google-gemini/gemini-cli`, immutable revision
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`. State: candidate reference inputs;
no runtime source or port is imported. Owner: P0-07 selection and P0-09 boundary
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
effects through those collaborators. P0-09 should express language-neutral
fixtures, compare exact semantic outcomes and implement bounded attributed Rust
adapters inside VCP's chosen engine. Preserve provider-independent behavior while
excluding SDK-specific types and implicit provider/telemetry routes. Any retained
code needs its complete selected dependency/license closure at import time.

Hooks and editor utilities remain deferred research. Node is comparison tooling;
this candidate does not add a second production engine or relax the OpenRouter,
local-embedding, one-store or one-ledger requirements.
