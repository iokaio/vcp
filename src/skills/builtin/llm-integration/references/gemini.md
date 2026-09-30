# Gemini application notes

VCP-authored. Load only for an application that already uses the Gemini API or Gemini on Google Cloud. This is a checked summary (2026-09-29) of the official function-calling and thinking guides and the `google-genai` Python types, not a model, price or limit table. Confirm names against the installed SDK; REST uses camelCase (`functionDeclarations`) and the Python SDK snake_case (`function_declarations`).

Checked against provider documentation on 2026-09-29; re-check before relying on version-specific details.

## Identify the API surface

Google documents `generateContent` (contents of `parts`) and a newer Interactions API whose function-call steps use a different shape (`function_call` / `function_result` with `call_id`). The same SDK can target the Gemini API (API key) or Google Cloud (Google Cloud credentials; see [hosting.md](hosting.md)). Keep the surface the application already uses.

## Function calling with `generateContent`

- **Declare:** `tools: [{"functionDeclarations": [{"name", "description", "parameters" | "parametersJsonSchema"}]}]`. `parameters` is the API's OpenAPI 3.0-style `Schema`; `parametersJsonSchema` is JSON Schema; they are mutually exclusive. See [tool-schemas.md](tool-schemas.md).
- **Model call:** a `model`-role content with one or more `functionCall` parts `{"id", "name", "args"}`; `args` is an object, not a string. Validate it before any effect.
- **Your result:** return the model content unchanged in history, then a content containing one `functionResponse` part per call: `{"id": <call id>, "name", "response": {...}}`. Put the result under an `output` key and failures under an `error` key. Content roles are `user` or `model`; check current examples for the role used with function responses.
- **Parallel and compositional calls:** several independent `functionCall` parts can arrive in one turn; answer each by ID. Dependent calls arrive across successive turns.
- **Modes:** `toolConfig.functionCallingConfig.mode` is `AUTO`, `ANY`, `NONE` or `VALIDATED`; allowed function names can be restricted. Check current docs for exact semantics.
- **Automatic function calling:** the Python SDK can call Python callables for you. If the application uses it, validation and authorization must run inside the callable before its effect; otherwise disable it and run a manual loop.

## Thinking and thought signatures

Parts can carry an opaque `thoughtSignature`. When the application sends full history itself (stateless), resend all thought parts and signatures exactly as received; do not remove, reorder or edit them, especially around function calls. Server-managed conversation state and SDK chat helpers can handle this for you; confirm which mode is in use before trimming history.

## Finish and safety outcomes

Check each candidate's `finishReason` before using output or calls: `STOP` is normal; `MAX_TOKENS` means truncated; `SAFETY`, `RECITATION`, `BLOCKLIST`, `PROHIBITED_CONTENT`, `SPII` and image variants mean content was withheld; `MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL` and `TOO_MANY_TOOL_CALLS` mean no executable call. A blocked prompt can return no candidates and set `promptFeedback.blockReason`; `safetyRatings` explain category scores. Treat an unrecognized value as incomplete, not as success.

## Other features

`countTokens` counts a request (including system instructions and tools) before sending, and responses report usage metadata. Batch mode accepts inline requests or a JSONL file where a user-defined key maps each response back to its request. Check current docs for limits and job states.

## Official starting points

- Function calling: <https://ai.google.dev/gemini-api/docs/function-calling>
- Thinking and signatures: <https://ai.google.dev/gemini-api/docs/thinking>
- API reference: <https://ai.google.dev/api/generate-content>
- Tokens: <https://ai.google.dev/gemini-api/docs/tokens>
- Batch: <https://ai.google.dev/gemini-api/docs/batch-api>
- SDK source: <https://github.com/googleapis/python-genai> and <https://github.com/googleapis/js-genai>
