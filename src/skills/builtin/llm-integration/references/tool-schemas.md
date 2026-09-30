# Wrapping a neutral tool schema

VCP-authored. [../assets/tool-schema-neutral.json](../assets/tool-schema-neutral.json) holds a provider-neutral definition: `name`, `description` and a plain JSON Schema object in `parameters` that rejects additional properties. `assets/tool-schema.json` is the same tool already wrapped for Anthropic; it is an on-demand reference derived from the neutral file, so read it only when needed. Keep one neutral source in the application and wrap it inside each provider adapter; do not copy a provider shape into shared code.

| Target | Wrapping |
| --- | --- |
| Anthropic Messages | `{"name", "description", "input_schema": parameters}` |
| OpenAI Responses | `{"type": "function", "name", "description", "parameters", "strict"}` (flat) |
| OpenAI Chat Completions | `{"type": "function", "function": {"name", "description", "parameters", "strict"}}` (nested) |
| Gemini `generateContent` | `tools: [{"functionDeclarations": [{"name", "description", "parametersJsonSchema": parameters}]}]` |
| Amazon Bedrock Converse | `toolConfig: {"tools": [{"toolSpec": {"name", "description", "inputSchema": {"json": parameters}}}]}` |

Adjustments the wrapper may need:

- **OpenAI `strict: true`:** every object needs `additionalProperties: false` and every property listed in `required`. Express an optional field as a null union, for example `unit` with `"type": ["string", "null"]` and `null` added to its `enum`, and treat `null` as "not supplied". Strict mode accepts only a subset of JSON Schema; check the current supported-keyword list before relying on keywords such as `minLength`/`maxLength`. With `strict: false`, the model output is not schema-enforced.
- **Gemini:** `parameters` takes the API's OpenAPI 3.0-style `Schema` object (for example uppercase `OBJECT`/`STRING` in some SDK examples); `parametersJsonSchema` takes JSON Schema. They are mutually exclusive. Check current docs for which JSON Schema keywords are honored.
- **Name rules differ** (allowed characters and maximum length). Keep names short, ASCII and underscore-separated so one name fits every target, and check provider limits on description length.

Provider enforcement is a convenience, not the trust boundary. The application still validates arguments against the neutral schema (and any semantic rules) before execution, because keyword support, strictness and hosted variants differ.
