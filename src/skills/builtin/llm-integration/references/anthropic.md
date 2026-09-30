# Anthropic application notes

Load only for an application that already selects Anthropic. Adapted from the pinned `claude-api` source in [../UPSTREAM.md](../UPSTREAM.md); this is a source snapshot, not a current model/price or compatibility table.

Use the project's installed Anthropic SDK and official documentation for its language. Python and TypeScript examples upstream use a Messages client, SDK streaming helpers and complete content blocks. Confirm concrete method names and supported parameters in the installed package before copying an example. Do not replace the application's selected model or endpoint from a default in upstream guidance.

The upstream [tool schema example](../assets/tool-schema.json) uses `name`, `description` and `input_schema`. Claude Messages tool requests use `tool_use` blocks, and corresponding `tool_result` blocks identify the request through `tool_use_id`. Preserve all required assistant content blocks and the documented tool-result ordering. Do not reduce an assistant turn to its text before continuing it.

Check the final `stop_reason`: an output limit or refusal can leave arguments incomplete. For provider-specific continuation reasons such as `pause_turn`, verify the SDK runner's behavior and resume only under a bounded continuation policy. Avoid adding invented user messages to force a continuation.

If the selected SDK exposes an automatic tool runner, validate inputs and enforce permissions inside the dispatch boundary before an effect. With manually assembled streaming arguments, guard both JSON parsing and schema validation; do not assume a tolerant partial parser means the arguments are complete.

Use the SDK's typed exceptions and final-message stream helpers instead of broad error-string matching or duplicate event assembly. Check structured-output parameter names, feature availability, token limits and caching syntax against the actual endpoint and model. An Anthropic-compatible gateway may support a different subset than the direct API.

VCP addition (checked 2026-09-29): with thinking enabled, pass every `thinking` and `redacted_thinking` block back complete and unmodified alongside its `tool_use` block when returning tool results. `/v1/messages/count_tokens` accepts the same inputs as a Messages request and returns an estimate. Message Batches results can arrive in any order and are matched by `custom_id`. Hosted platforms support a subset of these features; see [hosting.md](hosting.md).

Official starting points when network access is available:

- Messages and tool use: <https://platform.claude.com/docs/en/build-with-claude/tool-use/overview>
- Streaming: <https://platform.claude.com/docs/en/build-with-claude/streaming>
- Prompt caching: <https://platform.claude.com/docs/en/build-with-claude/prompt-caching>
- SDK source: <https://github.com/anthropics/anthropic-sdk-python> and <https://github.com/anthropics/anthropic-sdk-typescript>

If documentation or an endpoint is unavailable, continue the local implementation supported by installed types/tests and state the remaining compatibility check. Do not install a CLI, access additional credentials or make paid requests as an implicit fallback.
