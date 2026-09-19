# P2 OpenRouter live smoke qualification

Status: passed September 19, 2026. This is the separately authorized P2-02
live compatibility smoke gate, not a quality comparison or Jev qualification.

`scripts/test-openrouter-live.ps1` queried the current authenticated OpenRouter
catalog and admitted two fixed Responses API calls using only the public synthetic
marker `VCP_SMOKE_OK`. The runner permits no retries, bounds each response to 64
tokens, computes a catalog-price worst case before dispatch and rejects a configured
cap above `$10`. It records actual provider usage/cost after each response and
stops admitting work when the remaining cap is insufficient. Credentials and
request headers are never written to evidence.

## Result

Command:
`pwsh -NoProfile -File scripts/test-openrouter-live.ps1 -SpendCapUsd 10`.

Both required current catalog arms passed through
`https://openrouter.ai/api/v1/responses`:

| Requested/served model | Input | Output | Observed cost | Result |
|---|---:|---:|---:|---|
| `openai/gpt-5.6-luna` | 15 tokens | 9 tokens | `$0.0000138` | completed; exact marker |
| `anthropic/claude-sonnet-5` | 26 tokens | 14 tokens | `$0.0001920` | completed; exact marker |

The catalog-price preflight maximum was `$0.0007872`; observed aggregate cost was
`$0.0002058`, below the authorized `$10` cap. The run exited 0. Manifest:
`artifacts/openrouter-live/5958f16e-bd24-4a3c-9c52-e967d6a6da02/manifest.json`,
SHA-256 `b0e42b9fc9028c6f5c1a7c752f95ea17d5558cbc0750183f3f6971c5e60df86f`.

An exploratory Gemini 3.8 Flash request returned HTTP 200 but exhausted the
64-token output allowance with mandatory reasoning and ended `incomplete`.
Disabling reasoning returned HTTP 400 because that endpoint requires it. Gemini
was therefore not substituted into the regular bounded marker gate; this evidence
does not advertise Gemini Responses compatibility under the selected bound.

## Scope

This result verifies current authentication, catalog visibility, two-model
Responses transport, requested/served identity, completion parsing and usage-cost
reporting for a tiny public input. It does not establish coding quality, provider
fallback, retry orchestration, cancellation/recovery or arbitrary schemas.
`typesafe/jev-1.13` was absent from the current catalog, but Jev is intentionally
owned by the later P6 decision-adapter qualification and is not a regular P2
runner or a requirement of this result.
