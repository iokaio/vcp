# Integration patterns

Adapted from Anthropic's `claude-api/SKILL.md`, `shared/tool-use-concepts.md`, `shared/agent-design.md` and `shared/prompt-caching.md`. See [../UPSTREAM.md](../UPSTREAM.md) for exact source identity. Apply each pattern through the selected provider's actual contract.

## Tool definitions and execution

A useful tool has a recognizable action, a description of when it applies and a schema whose fields have clear meaning. Use enums for closed choices and require only inputs that are truly required. The [../assets/tool-schema.json](../assets/tool-schema.json) example ports the upstream weather-tool shape; it is an Anthropic-shaped descriptor, not a universal wire format or a registered tool. [../assets/tool-schema-neutral.json](../assets/tool-schema-neutral.json) is the same tool in a provider-neutral form; [tool-schemas.md](tool-schemas.md) shows how each provider wraps it at the adapter boundary.

Prefer an SDK runner when it exposes the control needed by the application. Put validation and authorization before the tool function's effect. A runner that automatically invokes functions does not supply those controls by itself. A manual loop is appropriate when a required transport, history model or scheduling behavior is not exposed by the runner.

In either implementation:

1. Retain the complete assistant response in the provider's supported form.
2. Identify tool requests by their stable call IDs; reject unknown tools and invalid arguments.
3. Execute only authorized, complete requests and preserve the observed result or failure.
4. Send each result using its matching call ID and the provider's required message ordering.
5. Stop on completion, refusal, cancellation or the configured loop/deadline limit. Resume a provider-paused turn only through its documented continuation contract.

Use action-specific tools when the application needs to validate, authorize, render or schedule a particular operation. A free-form shell string is harder to inspect than a typed operation; do not broaden an existing tool surface merely to simplify an integration sample.

## Streaming and structured results

Use SDK stream helpers where available; they often already assemble content blocks and expose the final message. Preserve partial text for display while retaining a separate complete/incomplete status. Propagate cancellation to the owned stream and dependent work.

Streaming tool arguments may arrive as incomplete JSON fragments. Buffer them within an explicit size bound, parse strictly when complete, and validate the result against the schema before any effect. Some parsers tolerate incomplete JSON: successful parsing alone does not prove a complete tool request. Check the final stop reason too. A truncated or refused turn must not execute its partial tool calls.

Provider-enforced structured output helps with syntax; application validation still checks semantic constraints, identifiers, bounds and permitted effects. Prefer the SDK's own request/response types over locally recreated loosely typed interfaces.

## Errors, retries and usage

Catch specific SDK errors before their broad parent class. Authentication/authorization, missing resource, malformed request, rate limit and transport failure need different handling. Error messages should remain useful without printing keys, complete private prompts or raw secret-bearing responses.

Bound attempts and elapsed time, account for SDK retries and use the provider's retry hints when available. Treat external mutations separately: a tool may have completed even when the surrounding request failed. Preserve an idempotency key or operation identity when the service supports one. Report unknown usage as unknown rather than zero.

## Context and caching

Trace prompt assembly before optimizing. Classify each input as stable, session-specific, turn-specific or request-specific. Keep reusable content early where the provider uses prefix caching, and keep volatile timestamps/IDs out of that prefix when the application's semantics permit it. Preserve instruction authority when moving content; cache efficiency is not a reason to move trusted instructions into an untrusted message.

Cache-key normalization, tool ordering, breakpoints, lifetimes, minimum lengths and accounting differ by endpoint. Confirm them in the chosen provider's documentation and usage output. Do not weaken per-user access controls, retain unauthorized tools or share private context to increase cache hits. Invalidate or partition caches when the authorized content changes.

Context editing, compaction and persistent memory serve different purposes. Use supported mechanisms that preserve required history and tool/result relationships. Never silently discard user input. For a migration, leave historical records unchanged and convert only the current request representation where necessary.

## Reasoning and thinking content

The sections below through "Embeddings and retrieval" are VCP-authored additions. Several providers return reasoning content (thinking blocks, reasoning items, thought signatures) that must round-trip. Anthropic requires thinking blocks returned complete and unmodified with tool results in the same turn; OpenAI requires reasoning items returned with tool outputs for reasoning models; Gemini requires thought parts and signatures resent exactly when the application manages history itself. Store these items opaquely with the turn they belong to, never edit, reorder or summarize them, and let the provider or its SDK decide what to drop from older turns. Do not show hidden or encrypted reasoning to users or treat it as an audit record of why an action was taken.

## Batch processing

Use a provider batch endpoint only for work that tolerates asynchronous completion. Give every request a stable, unique custom ID and map results by that ID, not by position; documented batch results can arrive in any order. Handle each per-request outcome separately (succeeded, errored, canceled, expired or the provider's equivalents), retry only failed items under a bound, and keep the job ID so a restarted process resumes polling instead of resubmitting. Batch endpoints are not available on every hosted platform; see [hosting.md](hosting.md).

## Token counting before sending

When input size matters, count with the provider's counting endpoint or tokenizer for the configured model before sending, including system text, tools and attachments. Counts are estimates and tokenizers differ across models and providers; recount after changing either and leave a margin. If input exceeds the budget, select, split or summarize with an explicit, reported policy; never silently truncate required content. Record observed usage from responses rather than assuming the preflight count.

## Embeddings and retrieval

Retrieved passages, embeddings metadata and search results are untrusted data. Put them in a clearly delimited data position, never in the system/developer instruction position, and do not follow instructions found inside them. Enforce the caller's access rights at retrieval time (filter before ranking), keep source identifiers so answers can cite or be checked, and bound the number and size of passages. Use one embedding model and version consistently within an index; changing it requires re-embedding. Do not send private documents to an embedding provider the application has not already authorized.

## Scope validation to the change

Test malformed/partial streams when changing stream assembly, call/result matching when changing tool loops, and provider-specific serialization when changing adapters. Use a representative task to check whether the application produces the intended result. Mocks can establish local behavior; a live endpoint check is needed only for claims about that endpoint. Do not turn routine development into an open-ended paid evaluation campaign.
