# Native Gemini boundary qualification

The P0-07 comparison runner executes 402 selected upstream tests on native
Windows. Gemini remains an external candidate: no TypeScript production engine
or attributed Rust port is imported. See [the evidence](../evaluations/p0-07-gemini-baseline.md),
[candidate responsibilities](../../src/third_party/components/gemini-cli.md) and
[ADR-013](../adr/013-upstream-reuse-and-vendoring.md).

## Inputs and explicit preparation

Use Node 24 or later, Git and VCP's pinned development tools. Acquire a fresh,
disposable checkout of `google-gemini/gemini-cli` at
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`, tree
`5cb35fa93b203b709e91737b0a00da985dbb186e`. From the VCP root:

```powershell
npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund
git init artifacts/upstream/gemini-candidate
git -C artifacts/upstream/gemini-candidate fetch --depth=1 https://github.com/google-gemini/gemini-cli.git 6a466a7e2fe2b1255752c1e74f69b31f0216084d
git -c core.longpaths=true -C artifacts/upstream/gemini-candidate checkout --detach 6a466a7e2fe2b1255752c1e74f69b31f0216084d
Push-Location artifacts/upstream/gemini-candidate
try { npm ci --ignore-scripts --no-audit --no-fund; if ($LASTEXITCODE -ne 0) { throw 'Gemini dependency installation failed' } }
finally { Pop-Location }
node scripts/upstream/test-gemini.cjs --source artifacts/upstream/gemini-candidate --output-root artifacts/gemini-qualification --prepare
```

Stop if an acquisition command fails. Installation uses the candidate's committed
lockfile and disables lifecycle scripts. The qualification command does not
fetch or install dependencies. `--prepare` runs only the inspected
`scripts/generate-git-commit-info.js`, then TypeScript `--build --force` in
`packages/core`. It does not run the upstream application or general package
build script. MCP tests resolve core through the upstream test rig and require
`dist/index.js`; generating Git metadata alone is insufficient.

Without `--prepare`, the runner requires both the generated
`packages/core/src/generated/git-commit.ts` and compiled core entry point before
running tests. Prefer `--prepare` for qualification evidence so compilation is
part of the recorded run. Vitest 3.2.4 and TypeScript 5.8.3 must match the lockfile.

## Execution and evidence contract

The command checks the manifest's exact commit/tree and a clean checkout before
execution and again after tests. Ignored dependency/generated outputs are allowed.
Git long-path handling and trust apply only to that command and explicit checkout.
Evidence must resolve outside the source, including through junction aliases.
Unknown or duplicate arguments fail before output allocation.

Each run creates a UUID evidence directory. Metadata, compiler and test stages
use the [delivery harness](delivery-harness.md): ten-minute stage limits, bounded
logs, process-tree cancellation and nonzero exit propagation. Children receive a
minimal environment with a fresh synthetic home/profile and the validated
`GIT_COMMIT` expected by the metadata generator. This excludes ambient provider
credentials and user configuration from normal discovery; it is environment
isolation, not an OS sandbox or network-denial guarantee.

The final manifest binds the VCP commit/dirty state, candidate identity, tool
versions, package-lock bytes, runner/harness/policy hashes, stage manifests and
Vitest JSON result hash. A pass requires all eight expected files and all 402
assertions to pass; missing, duplicate, filtered, skipped or failed results fail
qualification. Missing prerequisites return `not_run`/3, failed checks return a
nonzero exit, and malformed invocation returns 2. Interrupted or incomplete runs
are never passes. Retain failed evidence as well as successful results.

The manual Windows qualification job explicitly acquires the pin and dependencies, then runs this
command on the standard `windows-2025` runner. Uploaded evidence contains
manifests, stage logs and the test report; it excludes synthetic profiles and
does not include the downloaded checkout or dependencies. Deterministic harness
regressions also run on `ubuntu-24.04` without acquiring Gemini.

## Selected semantics and adaptation instructions

All paths below are relative to the pinned `packages/core`. Tests use mocked
SDKs, transports and collaborators. Counts describe this immutable candidate.

| Test path | Tests | Behavior and required VCP adaptation |
|---|---:|---|
| `src/policy/policy-engine.test.ts` | 155 | Decision precedence and matching. Enforce the VCP permission ceiling before admitting effects. |
| `src/scheduler/scheduler.test.ts` | 43 | Queues, cancellation, confirmation and result lifecycle. Fence dispatch under the root controller and reconcile partial effects. |
| `src/tools/tools.test.ts` | 15 | Validation/build and execution failure mapping. An execution error cannot prove an effect did not happen. |
| `src/policy/stable-stringify.test.ts` | 21 | Stable policy-match encoding. Do not adopt it as a canonical JSON or durable argument-hash format. |
| `src/skills/skillLoader.test.ts` | 15 | Filesystem discovery and metadata parsing. Apply workspace identity, scope and provenance before instruction promotion. |
| `src/skills/skillManager.test.ts` | 10 | Built-in, extension, user and workspace precedence. VCP owns trust and override rules. |
| `src/tools/mcp-client.test.ts` | 78 | Discovery, transports, authentication and progress lifecycle. Route process/network/auth effects through explicit VCP authority. |
| `src/tools/mcp-tool.test.ts` | 65 | Tool adaptation and result/error handling. Bind prepared requests, cancellation and receipts to the owning task. |

`stable-stringify.ts` sorts nested keys but wraps top-level properties with literal
NUL separators for matching. It handles cycles with a marker and omits or
normalizes unsupported values, can invoke `toJSON`, and rejects BigInt. P0-09 must
compare its intended matching behavior while keeping canonical protocol encoding
separate. See [the engine design](../architecture/engine-execution-design.md).

`scheduler/policy.ts` can turn an ask decision into allow for client-initiated
calls without additional permissions; persisted always-allow choices can change
later decisions. Preserve these observations as comparison cases, then document
VCP's intentional divergence: neither client origin nor upstream remembered
approval can exceed current VCP authority. `/pause` must fence root and child
dispatch while keeping the CLI open; upstream abort alone does not satisfy
[ADR-016](../adr/016-history-and-pause.md).

`skillLoader.ts` reads and parses files; `skillManager.ts` combines multiple
discovery roots. `mcp-client.ts` can start stdio transports, make network requests
and discover/refresh credentials. Before porting, enumerate each retained effect,
replace implicit discovery with injected authority, and test cancellation during
each effect. Follow [the extensions design](../architecture/routing-extensions-design.md)
and P0-09's language-neutral fixtures. No production extraction is qualified by
these mocked tests; retained code still needs dependency/license closure and
attributed modification records.
