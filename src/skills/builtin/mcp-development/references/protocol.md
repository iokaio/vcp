# MCP protocol and design patterns

Adapted from Anthropic's `mcp-builder/reference/mcp_best_practices.md`, `node_mcp_server.md` and `python_mcp_server.md`; exact sources and modifications are in [../UPSTREAM.md](../UPSTREAM.md). Protocol facts below were checked on 2026-09-29 against the published specification, Python `mcp` 2.2.0 (with `mcp-types` 2.2.0) and TypeScript `@modelcontextprotocol/sdk` 1.31.0. Treat them as a starting point: the installed version's types and docs win. SDK-specific code is in `references/python.md` and `references/typescript.md`.

## Protocol revision and SDK versions

The newest dated MCP specification revision is `2026-07-28` (<https://modelcontextprotocol.io/specification/2026-07-28>). It replaces the `initialize` handshake with stateless, self-contained requests and per-request capability negotiation, and deprecates the logging capability. Earlier revisions are `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05`.

SDKs negotiate the version; do not hard-code it in tool logic. Python `mcp` 2.2.0 speaks `2026-07-28` and every earlier revision (`mcp_types.LATEST_PROTOCOL_VERSION`). TypeScript SDK 1.31.0 tops out at `2025-11-25` (`LATEST_PROTOCOL_VERSION` in `@modelcontextprotocol/sdk/types.js`). Check the peer's negotiated version before relying on a newer feature.

## Schemas

Use the project's MCP SDK instead of hand-writing protocol framing. Confirm the installed SDK's accepted schema shape: a raw JSON Schema, a Zod field map and a Zod object are not interchangeable across all interfaces or versions.

A user search input can be expressed with these language-independent constraints:

```json
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "query": {"type": "string", "minLength": 2, "maxLength": 200},
    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20},
    "offset": {"type": "integer", "minimum": 0, "default": 0}
  },
  "required": ["query"]
}
```

Document whether omitted values receive defaults; JSON Schema's `default` keyword alone does not populate them. Reject unexpected fields where the SDK supports strict models. Avoid coercing booleans, fractions or numeric strings into pagination counts. Let the SDK expose the generated input schema, then inspect that schema from a client.

## Annotations

Describe the tool's effects separately from its data shape. The annotations are `readOnlyHint`, `destructiveHint`, `idempotentHint` and `openWorldHint` (Python: `read_only_hint` and so on). A read operation against an external API can be read-only while still interacting with an open world. Clients treat annotations from untrusted servers as untrusted, and the service must enforce permissions regardless of annotation values.

## Tools, resources and prompts

- **Tools** are model-invoked actions or queries with arguments, effects or network calls. Most service operations belong here.
- **Resources** are addressable, read-only context that the host or user selects by URI, such as a file, a record or a schema. Do not use a resource for an operation with side effects.
- **Prompts** are user-selected message templates for a recurring workflow. They are not a substitute for tool descriptions.

Add resources or prompts only when a client in use surfaces them and the task benefits. Tools remain the portable default.

## Output schemas and structured content

Prefer structured fields for data a client needs to act on. When a tool declares an `outputSchema`, its `structuredContent` must conform. Also return a text block serializing the same data for clients that read only content. Supply human-readable Markdown when it has a real consumer instead of adding a mandatory output-format option to every tool. Keep IDs alongside display names so that follow-up calls can select the intended resource. Omit irrelevant private fields.

## Pagination

For offset pagination, return the actual count, whether more results exist and a next offset:

```json
{
  "total": 3,
  "count": 2,
  "offset": 0,
  "items": [{"id": "U1"}, {"id": "U2"}],
  "has_more": true,
  "next_offset": 2
}
```

For cursor APIs, preserve the service's opaque cursor; do not invent an offset or total. Apply sensible default and maximum page sizes, and never silently discard excess results.

## Elicitation

Elicitation lets a server ask the user for input during a tool call. It requires the client's elicitation capability; do not assume one. Use form mode only for non-sensitive values such as a confirmation, choice or missing parameter. Use URL mode for secrets, OAuth consent or payments, so that they never pass through the client or model. Handle accept, decline and cancel explicitly. An accepted value is still untrusted input: validate it. A confirmation obtained this way does not replace the host's own approval or the service's authorization.

## Errors and cancellation

For a valid tool invocation that fails, return the SDK's tool error result (`isError` in the protocol representation). Keep protocol errors for malformed requests, unknown methods and missing capabilities. "Resource not found; check the identifier" and "Permission denied for this workspace" give a client useful direction. Keep raw exception details and secrets out of the result. Authentication failure, no matches, rate limits and partial results should remain distinguishable.

Honor the service's retry guidance within a deadline and attempt limit. A read timeout may be retryable. A creation timeout can leave an unknown effect: reconcile through the operation identity or idempotency key before retrying. Propagate cancellation to owned requests and close owned resources. Return an operation identifier if the service needs later reconciliation.

## Transports

Stdio fits a locally spawned server. Keep human logs on stderr; even a startup banner or stray `print` on stdout can corrupt the protocol. Streamable HTTP fits hosted or multi-client servers. HTTP+SSE is kept only for backward compatibility. Use the transport the client actually requires rather than changing an existing deployment because a sample prefers another one.

For HTTP, bind local development to loopback and use the SDK's origin/DNS-rebinding protection. Protect session state. Tool-supplied paths or URLs do not expand allowed filesystem roots or destinations.

## Authorization

For HTTP servers that need user authorization, follow the spec's Authorization section for the negotiated revision. The server acts as an OAuth 2.1 resource server. It publishes protected-resource metadata that names its authorization server, and it validates each bearer token's audience (resource indicator) and scopes before dispatch. Do not pass the client's token through to upstream APIs; obtain separate upstream credentials.

Stdio servers take credentials from their environment or configuration, not from an OAuth flow. MCP authorization never widens what the host allows: host approval, VCP permissions and the service's own authorization still apply.

## A useful development check

From a separate client, discover the tools, search a known fixture, follow a second page and use a returned ID in a detail request. Assert the actual expected objects, `structuredContent` and effects. Add failure cases at the changed boundary: invalid arguments, an `isError` path, a denied operation and a declined elicitation where relevant. Then run the normal project checks.

For interactive inspection, use the MCP Inspector: `npx @modelcontextprotocol/inspector` for the web UI, or add `--cli` for scripted checks. Downloading or running it requires the corresponding existing authorization. Tests with a synthetic service need no paid model calls and do not claim compatibility with an untested production endpoint.
