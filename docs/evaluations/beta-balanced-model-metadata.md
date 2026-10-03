# BETA-03D balanced set: public metadata inspection

On October 2, 2026, read-only public OpenRouter requests confirmed the balanced
set's exact model identifiers and endpoint fields. No credential was supplied,
no inference was performed and no model quality or live conformance was tested.

The relevant admission rules were inspected in `vcp-models`' endpoint parser and
reusable compatibility contract: exact endpoint identity, no expanding base
provider aliases, active status, tools/tool-choice/output-limit parameters,
positive context/output bounds and supported numeric tariff fields. Counts below
are from inspecting those fields; this inspection did not run the Rust parser
against the downloaded responses.

| Public endpoint catalog | Rows | Rows meeting inspected gates | Context / maximum output | Raw UTF-8 bytes |
| --- | ---: | --- | --- | ---: |
| [openai/gpt-4.1-mini](https://openrouter.ai/api/v1/models/openai/gpt-4.1-mini/endpoints) | 3 | `openai` | 1,047,576 / 32,768 | 3,281 |
| [openai/gpt-4.1](https://openrouter.ai/api/v1/models/openai/gpt-4.1/endpoints) | 3 | `openai` | 1,047,576 / 32,768 | 3,195 |
| [qwen/qwen3-coder](https://openrouter.ai/api/v1/models/qwen/qwen3-coder/endpoints) | 5 | All five exact endpoint tags | 256,000–262,144 / 65,536 | 5,419 |

Both OpenAI catalogs' Azure rows omit `max_tokens`; their base `azure` tags also
expand to a listed regional endpoint. They therefore do not meet this adapter's
current exact endpoint contract. Qwen's rows are `google-vertex/us-south1`,
`deepinfra/turbo`, `venice/fp8`, `novita/fp8` and `alibaba/opensource`.
The Alibaba row has supported context-price tiers; the parser conservatively
uses their maximum rates. The Google Vertex row has the lowest conservative
128-output-token reservation among those inspected Qwen rows.

With 2,048 output tokens, the existing full-input/cache reservation convention
and the USD 0.001 request ceiling give approximately USD 1.261370 for the mini
model, USD 6.302840 for GPT-4.1 and USD 0.177703 for Qwen's Google Vertex row.
These are admission reservations, not observed charges. The initial default task
budget is USD 10 so each proposed primary role can fit this inspected metadata;
remaining task budget still controls every dispatch and eligible fallback.

The three raw catalogs total 11,895 bytes before JSON string escaping. Exact
production profile serialization size was not measured in this read-only check;
profile publication retains its 256 KiB bound. Synthetic tests cover profile
materialization, role selection and limits. The metadata is a dated observation,
not a checked-in provider catalog: setup and new tasks fetch fresh endpoint data.

## Maker and project suggestions

The following primary OpenRouter descriptions were read on October 2, 2026 to
explain choices in the interview. Project groupings are VCP's suggested use of
the stated strengths, not measured project-specific rankings:

- [GPT-4.1 Mini](https://openrouter.ai/openai/gpt-4.1-mini) describes lower-cost,
  responsive coding; it handles everyday implementation and support in balanced.
- [GPT-4.1](https://openrouter.ai/openai/gpt-4.1) describes instruction following,
  software engineering, repository context and precise code diffs; it handles
  review and verification in balanced and leads the systems suggestion.
- [Qwen3 Coder](https://openrouter.ai/qwen/qwen3-coder) describes code generation,
  tool use and repository context; it leads the web suggestion and supplies a
  coding-focused alternative in mixed sets.
- [Sonnet 4.5](https://openrouter.ai/anthropic/claude-sonnet-4.5) describes coding,
  system design, security and specification adherence; it leads the backend and
  Anthropic suggestions and reviews the systems suggestion.
- [Haiku 4.5](https://openrouter.ai/anthropic/claude-haiku-4.5) describes responsive
  coding and support-agent use; it supports Sonnet in the Anthropic set.
- [Gemini 2.5 Flash](https://openrouter.ai/google/gemini-2.5-flash) describes coding,
  mathematics and scientific tasks; it leads the Google and data suggestions.

Read-only endpoint API requests additionally confirmed exact IDs for Sonnet 4.5,
Haiku 4.5 and Gemini 2.5 Flash. Sonnet listed seven rows with active status,
tools, tool choice, output limits and positive bounds; `amazon-bedrock` is a
base alias with a regional row and is excluded by VCP. `azure/global` has a
200,000-token context and lower conservative reservation than the million-token
Sonnet rows. Haiku listed eight rows meeting those inspected metadata fields.
Flash listed seven rows, six meeting those fields; its `google-ai-studio` base
alias expands and is excluded. These counts are field inspection, not Rust
parser execution or live Responses evidence. Fresh runtime admission remains
authoritative.

[Gemini 2.5 Pro's endpoint catalog](https://openrouter.ai/api/v1/models/google/gemini-2.5-pro/endpoints)
was also inspected. Its otherwise eligible rows include audio tariff override
fields outside the current parser contract, so it is deliberately absent from
the suggestions. No pricing or compatibility gate was relaxed to add a maker.
The single-model Google and Qwen sets explicitly explain that they have no
different-model fallback. Users can customize roles or choose a mixed set.
