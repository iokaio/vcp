# P10-01 G04 upstream hook evidence

On 2026-09-24, the pinned Gemini registry/planner/runner qualification passed
59 assertions across three suites on native Windows `10.0.26200`, Node `24.21.0`,
TypeScript `5.8.3` and Vitest `3.2.4`. Metadata preparation, core compilation and
test execution each passed. The checkout matched commit
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`, tree
`5cb35fa93b203b709e91737b0a00da985dbb186e`, and was clean before and after.

Command:

```powershell
node scripts/upstream/test-gemini-hooks.cjs --source artifacts/upstream/gemini-candidate --output-root artifacts/gemini-hooks-qualification --prepare
```

Local manifest:
`artifacts/gemini-hooks-qualification/a3336f9a-fe4b-4548-94c4-1a20b50e17a3/manifest.json`.
The run records VCP base `0294545002df46c118926e3f77ef1c65279a26cc` with a dirty
working tree; it is implementation evidence, not a claim about the final merged
commit. The manifest and stage evidence remain in ignored artifacts.

| Bound input/result | SHA-256 |
|---|---|
| Selected upstream fixture | `8eeed63524e02a76c42ea3a3d743a12df7052bc2cb8490574188c7e59db7c59b` |
| Pinned package lock | `7d5676779f732790ad6f8fe0610401c8a3df436e7af4feec04c830ca049f8511` |
| Vitest JSON result | `027284fcf5eaf04369f448c6af3c398b0a62a3c0264cafc14800a2e7fda5ef40` |

The validator regression command
`node --test src/tests/contracts/gemini-baseline.test.cjs` passed all six tests,
including separation from the historical 402-test P0-07 profile and rejection of
incomplete G04 assertions.

Retained unsuccessful runs: `d601d82e-b35f-4c8e-a8cf-38b66d4a123d` failed before
execution on repository ownership; `3479a826-ebff-4229-92be-c6093358fbec` failed
source hashing because the isolated Git environment omitted an ambient trust
override. Native execution under the repository owner resolved these without
changing global Git configuration. `c23926c1-eb90-4c8d-8f21-8c496e727f47` passed all
59 upstream assertions but correctly failed qualification because the preliminary
fixture count expected 58. The final fixture records the verified 25 runner
assertions, including its parameterized case; no upstream test was changed.

These mocked upstream suites do not prove VCP execution containment, durable
recovery or authorization. The [adaptation record](../development/p10-hooks-upstream.md)
separates upstream behavior from the required VCP semantics under ADR-014.
