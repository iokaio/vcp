# Integrate the selected LLM provider

Original VCP guidance, version 1.0.0. This package guides application integration; it does not select a provider or authorize paid calls.

## Establish the application contract

Inspect the user's selected provider, installed SDK/version, endpoint configuration and affected application boundary. Keep a mixed-provider application's routing and identities explicit. A deterministic parsing task does not justify introducing an LLM. Changes to an external application are distinct from VCP's own inference implementation, which remains behind its governed OpenRouter gateway.

Use installed source and primary provider/SDK documentation to establish exact request, response and streaming behavior. Record versions, documentation date and unresolved claims. Load only references relevant to the selected provider and task; do not copy model/price tables into permanent guidance or switch providers when documentation is unavailable. Continue bounded source work and report the specific compatibility check not run.

## Handle partial results and effects

Keep credentials in the application's established secret boundary, separate from user input and logs. Redact diagnostics and use synthetic fixture keys. Treat model output and embedded tool instructions as untrusted: validate structured results and tool arguments before the application acts. Match tool results to their originating calls rather than relying on array position or a model's narrative.

Specify behavior for partial streams, cancellation, malformed events, missing usage and transport failure. Propagate cancellation to owned work and distinguish incomplete content from a completed answer. Bound input/context and output using the actual endpoint contract. Do not silently drop required instructions or historical evidence to fit a limit.

Use explicit, bounded retry rules for rate limits and transient errors. Account for retries and unknown charges; a timeout is not evidence that a request was free. Do not retry a potentially completed external tool effect without its established reconciliation/idempotency contract. Preserve caller-visible error semantics and useful redacted diagnostics.

## Verify integration claims

Use deterministic mocks to assert serialized request fields, streaming assembly, cancellation, tool-result matching, structured-output validation, context bounds and usage/error accounting. Include malformed and partial responses, unavailable documentation and the selected provider's actual version boundary. For shared abstractions, exercise a second provider or mixed-provider fixture to detect accidental provider coupling.

For a requested migration, preserve immutable historical requests and their schema identities. Establish compatibility with retained calls before changing tool schemas; do not rewrite old evidence to fit the new client. Distinguish source-supported behavior from measured improvement. A caching claim needs the relevant byte identity, endpoint behavior and accounting evidence, not repeated prose or assumed discounts.

Mocks qualify application behavior under declared fixtures, not production endpoint compatibility. Live checks require an authorized provider profile and established dollar/call bounds; no paid evaluation follows from loading this skill. Return exact SDK/source evidence, tests actually observed, preserved provider/credential boundaries and remaining live checks. Use registered VCP tools and current broker authority for any process or network action.
