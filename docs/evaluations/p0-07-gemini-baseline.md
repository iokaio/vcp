# P0-07 — Reproducible Gemini boundary tests

Status: 402 selected upstream tests pass on native Windows through the new
qualification command. P0-07 remains `in_progress`; P0-09 ports and VCP lifecycle,
permission, local-memory and release acceptance remain outstanding.

## Inputs and observations

The [procedure and source map](../development/gemini-baseline.md) bind candidate
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`, tree
`5cb35fa93b203b709e91737b0a00da985dbb186e`, to eight selected suites. No upstream
runtime source is imported or modified. The package-lock SHA-256 is
`7d5676779f732790ad6f8fe0610401c8a3df436e7af4feec04c830ca049f8511`.

The earlier [213-test baseline](p0-07-native-candidates.md) covered policy,
scheduling and tools. A follow-up selected 189 additional argument-encoding,
skill and MCP tests. Its first run passed 111 assertions but failed to load the
MCP client suite: the upstream test rig imported the unbuilt core package.
Record: `artifacts/upstream/gemini-g06-3adfe916-0433-449b-ba6f-976fabafd50c`;
log SHA-256 `81695b50d66612af0d3938e241befb4ed8f0ca2fafe8d706721f0d20685e62c5`.

Explicitly running `node ../../node_modules/typescript/bin/tsc --build` from
the candidate's `packages/core` succeeded. The retry passed all 189 additional
tests in five files, September 18, 2026, 00:26:57–00:27:28 UTC:
`artifacts/upstream/gemini-g06-2875e4d6-57df-4abf-b041-1f2ed2b9b4c3`;
log SHA-256 `34d7b929122c1f23f35d59dcb432a72c62e21604e76e04d25622e05b42acd739`.
The failure is retained; no assertion was disabled to obtain the pass.

## Implemented runner

```powershell
node scripts/upstream/test-gemini.cjs --source artifacts/upstream/gemini-6a466a7e2fe2 --output-root artifacts/upstream/gemini-runner --prepare
```

The first complete command run passed all 402 tests in eight files, including
metadata generation and a forced TypeScript rebuild. Evidence:
`artifacts/upstream/gemini-runner/68fe4410-447e-432c-a150-0ff68dc58b2a/manifest.json`.
It ran September 18, 2026, 01:18:29–01:19:33 UTC on Windows 10.0.26200 with
Node 24.10.0, Git 2.51.0.windows.1, Vitest 3.2.4 and TypeScript 5.8.3. VCP base
was `4637c09b0975d3cb576c17e76374d80aafd05cc1` with uncommitted implementation.
Result SHA-256:
`4dd710b68ce367aae85a10bc2f0a9530c591177b99671da7ce8bdce11fb0a1d9`.
The manifest retains the exact runner, harness and policy hashes for that run.

After adding explicit missing-development-tool handling, the final runner again
passed all 402 tests and both preparation stages:
`artifacts/upstream/gemini-runner/9caf569c-782a-4eb7-a734-c45f805f4a5e/manifest.json`.
The final fast command, `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`,
passed 49 regressions plus repository/source/boundary checks:
`artifacts/tests/e2098970-6cc8-41b1-8b2a-58bea9573bba/manifest.json`.
These runs used the same VCP base with uncommitted changes; hosted CI validates
the committed PR inputs separately.

Regressions exercise wrong-source rejection before preparation, missing-tool
`not_run`, direct/junction output containment, strict arguments, complete result
validation and a real child process with an isolated home and environment.
The ordinary fast suite includes these regressions without invoking Gemini.

CI runs the full command on `win8core`, with explicit pinned acquisition and
dependency installation. Local success is separate from the PR's hosted result;
merge requires both delivery jobs to pass for the published head.

## Limits and next integration work

These are upstream unit tests with SDK/transport/filesystem collaborators mocked
as selected by upstream. They do not prove OS enforcement, a network-disabled
runtime, live provider behavior, durable effect receipts or working VCP skills
and MCP. Synthetic profile isolation excludes ordinary ambient configuration
discovery but does not sandbox arbitrary source access. No paid calls were made.

P0-09 must record semantic fixtures and intentional VCP divergences before ports.
The development guide identifies the policy encoding's noncanonical behavior,
client-initiated approval bypass, skill discovery precedence and MCP credential
effects. The root controller, current authority and durable pause/reconciliation
contracts remain governed by the architecture, including `/pause` without exit.
