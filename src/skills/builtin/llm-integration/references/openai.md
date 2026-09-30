# OpenAI application notes

VCP-authored. Load only for an application that already uses the OpenAI API or an OpenAI-compatible endpoint. This is a checked summary (2026-09-29) of the official function-calling guide and the `openai-python` types, not a model, price or limit table. Confirm names against the installed SDK.

## Pick the API the application already uses

OpenAI has two request shapes with tool calling: the **Responses API** and **Chat Completions**. Do not migrate between them as a side effect of another change; the current guide notes that some models support tool calling only through Responses, so check current docs when a feature is missing.

| | Responses | Chat Completions |
| --- | --- | --- |
| Tool definition | flat `{"type": "function", "name", "description", "parameters", "strict"}` | nested `{"type": "function", "function": {"name", "description", "parameters", "strict"}}` |
| Model's call | output item `{"type": "function_call", "call_id", "name", "arguments"}` | assistant message `tool_calls[]` entries `{"id", "type": "function", "function": {"name", "arguments"}}` |
| Your result | input item `{"type": "function_call_output", "call_id", "output"}` | message `{"role": "tool", "tool_call_id", "content"}` |
| End state | `status` (`completed`, `incomplete`, `failed`, `cancelled`, ...) and `incomplete_details.reason` | `finish_reason`: `stop`, `length`, `tool_calls`, `content_filter` |

`arguments` is a JSON **string** in both APIs. Parse it strictly, then validate it against the schema before any effect.

## Call/result pairing and ordering

- **Responses:** append the model's output items to the next request's input (or continue with `previous_response_id` / a conversation object when the application uses server-side state), then add one `function_call_output` per `call_id`. For reasoning models, reasoning items returned alongside tool calls must also be passed back with the tool outputs. When not using server-side state, check the conversation-state docs for how encrypted reasoning content is requested and returned.
- **Chat Completions:** append the assistant message containing `tool_calls` unchanged, then one `tool` message per `tool_call_id`, before any new user message. Send a result for every call ID; check current docs for how missing or unknown IDs are rejected.
- Match results by ID, not by position. Return a failed or rejected call as an explicit error result for that ID rather than dropping it.

## Parallel tool calls

A single turn may contain several calls. `parallel_tool_calls: false` limits a turn to zero or one call. When parallel calls are allowed, run calls concurrently only if they are independent and each passes authorization; otherwise execute sequentially. Either way, send every result keyed by its ID.

## Strict schemas

`strict: true` constrains arguments to the schema, subject to documented requirements: the root is an object (not `anyOf`), every object sets `additionalProperties: false`, every property is listed in `required`, and optional values use a `null` union. Only a subset of JSON Schema keywords is supported and there are size and nesting limits; check the current Structured Outputs documentation rather than assuming a keyword is enforced. See [tool-schemas.md](tool-schemas.md) for converting the neutral schema.

## Official starting points

- Function calling: <https://developers.openai.com/api/docs/guides/function-calling>
- Structured Outputs: <https://developers.openai.com/api/docs/guides/structured-outputs>
- Conversation state: <https://developers.openai.com/api/docs/guides/conversation-state>
- Batch: <https://developers.openai.com/api/docs/guides/batch>
- SDK source: <https://github.com/openai/openai-python> and <https://github.com/openai/openai-node>

An OpenAI-compatible gateway or local server may accept the request shape but ignore `strict`, parallel control or reasoning items. Test the behavior the application relies on against that endpoint, or report the check as not run.
