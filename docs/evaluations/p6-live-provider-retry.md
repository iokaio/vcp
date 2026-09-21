# P6 live provider retry — September 21, 2026

The updated saved credential passed the read-only authentication check (HTTP
200). Both candidate models subsequently completed the fixed echo-tool call and
exact final-text continuation through the real Responses API. Authentication is
no longer the blocker. This is protocol evidence, not completed P6 qualification.

The first fresh retry still used the original binary and returned HTTP 404:
`No endpoints found that can handle the requested parameters`, with the routing
failure at `Filter by Parameters`. The exact Bedrock catalog advertised tools,
tool choice and output limits, but not `parallel_tool_calls`. The encoder and
probe unconditionally sent that optional parameter. The production encoder now
emits it only when explicitly present in the catalog-validated compatibility
requirements; the single-echo probe omits it and checks `tool_choice` alongside
`tools` and `max_tokens` before dispatch. Strict parameter support, provider
restrictions, privacy settings and monetary ceilings remain enforced.

| Invocation | Requests | Result | Settled ledger USD | Unresolved USD |
|---|---:|---|---:|---:|
| [Original attempt](p6-live-provider-conformance.md) | 1 | HTTP 401 | 0 | 0.201640 |
| Fresh authenticated retry, original binary | 1 | HTTP 404 parameter filter | 0 | 0.201640 |
| Corrected `anthropic/claude-3-haiku`, `amazon-bedrock` | 2 | Tool and continuation observed | 0.000319 | 0 |
| Corrected `anthropic/claude-haiku-4.5`, `anthropic` | 2 | Tool and continuation observed | 0.001736 | 0 |
| **Total under the authorized $25 cap** | **6** | | **0.002055** | **0.403280** |

Every invocation used a fresh permanent claim, a $1 root cap, at most 512 output
tokens, and zero retries. The settled ledger rounds each observed charge upward
to an integral microdollar: the raw successful charges total $0.00205475.
Unknown charges remain reserved, so $0.405335 of the authorization is accounted
for and $24.594665 remains before any further allocation. No task-quality run
has started and no previous claim was replayed.

All four completed Responses omitted served-provider identity. Separate
read-only generation lookups named Amazon Bedrock and Anthropic, respectively,
and returned stable endpoint UUIDs within each pair. The public captured
catalogs do not map those UUIDs to exact endpoint tags. The stronger generation
metadata also names `anthropic/claude-4.5-haiku-20251001`, while the Responses
payload names the requested `anthropic/claude-haiku-4.5` alias. These are retained
observations; neither provider display names nor a requested pin are silently
promoted into exact endpoint qualification. Both reports retain
`provider_preferences_qualified: false` and `byte_ceiling_qualified: false`.

The [provider-routing documentation](https://openrouter.ai/docs/guides/routing/provider-selection)
defines strict parameter filtering. The [generation metadata API](https://openrouter.ai/docs/api/api-reference/generations/get-request-&-usage-metadata-for-a-generation)
provided the supplemental attribution and charges; it did not supply the missing
catalog-tag mapping. Raw metadata remains private alongside the synthetic probe
captures; no credentials or account metadata are published here.

## Reproduction and identities

The corrected executable SHA-256 is
`578de972e410409df349d4c8397df74c5ca10c1be27553fa441d4c250f0b97eb`.
Each claim also retains the compiled source identities. The original-binary retry
used the binary recorded in the first-attempt report.

| Invocation | Authorized spec SHA-256 | Result SHA-256 |
|---|---|---|
| Authenticated parameter rejection | `9b35000e64296d756c0620d19a80bb3951ccd748701ca2f9ea09b151b092d8b2` | `10a9135526858b780c66fb8eab5bf2817b9a24bcc6ba5f81b3dfeeb10b3664a0` |
| Corrected economical | `4c10486c6a925482b73d2cd9cb452a2894c409c9d42429eab9ab887c5936b26e` | `7c9ee6650eeb8b44d72963f3f2d1524b46afcc7b9ee404570904f0a53c930c06` |
| Corrected stronger | `07ee5f354a8ad73b456b4c856d8f7562b2b13b0f213488b9f6891dfba5ac9677` | `e41630ab7646de662a008d82ce0bd50074e54889550ef4e6323445e373343618` |

The economical raw catalog SHA-256 remains
`abc476ae65e98c175ea379bd7d166a1235893b8f842c48de722f70372e1c0088`;
the stronger catalog is
`1560d2f07a4c0263aacd15893b21a02a98ca463bb07a1b9fc5ce17a020414e49`.
Ignored local pointers are `artifacts/p6-live-{retry,fixed,stronger}-paths.json`;
aggregate accounting remains `artifacts/p6-live-authorization.json`.

Local validation: all 75 model tests passed, including optional-parameter
qualification and rejection coverage. The conformance local-peer test passed
with strict provider flags, two observed settlements and unknown-cost stopping.
The executable rebuilt successfully; the repository fast suite passed all 11
cases. Formatting and Clippy passed (existing warnings remain). Independent review
found no blocking issues.

## Remaining acceptance

The [task runner](../../scripts/evals/p6-live-runner.md) requires qualified
provider snapshots; these probes do not yet provide exact endpoint qualification.
Automatic routing also retains its qualified-byte gate. Profile quality, actual
Jev/conventional evaluator comparisons, held-out advisory qualification and
measured defaults remain incomplete under [P6's contract](../plan/12-routing-and-optimization.md).
Even successful execution of the six-case smoke corpus would not establish the
larger sample and severity gates. No shipping groups or defaults were enabled.
