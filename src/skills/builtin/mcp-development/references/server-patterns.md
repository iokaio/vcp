# MCP implementation patterns

Adapted from Anthropic's `mcp-builder/reference/mcp_best_practices.md`, `node_mcp_server.md` and `python_mcp_server.md`; exact sources and modifications are in [../UPSTREAM.md](../UPSTREAM.md).

## Schemas and registration

Use the project's MCP SDK instead of hand-writing protocol framing. In TypeScript, upstream demonstrates `McpServer`, `server.registerTool` and Zod. In Python, it demonstrates `FastMCP`, `@mcp.tool` and Pydantic models. Confirm the installed SDK's accepted schema shape: a raw JSON Schema, Zod field map and Zod object are not interchangeable across all interfaces or versions.

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

Describe the tool's effects separately from its data shape. Useful annotations include `readOnlyHint`, `destructiveHint`, `idempotentHint` and `openWorldHint`. A read operation against an external API can be read-only while still interacting with an open world. The service must enforce permissions regardless of annotation values.

## Return focused, useful results

Prefer structured fields for data a client needs to act on. A concise text representation can help clients that display text results; supply human-readable Markdown when it has a real consumer instead of adding a mandatory output-format option to every tool. Keep IDs alongside display names so follow-up calls can select the intended resource. Omit irrelevant private fields.

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

The Python helper [../assets/paginated_result.py](../assets/paginated_result.py) implements this response for already bounded pages. It rejects invalid limits, oversized pages, inconsistent totals and empty pages that would never advance. The service adapter must still bound its response body and validate each item before calling it. For cursor APIs, preserve the service's opaque cursor; do not invent an offset or total. Apply sensible default and maximum page sizes, and never silently discard excess results.

## Errors and cancellation

For a valid tool invocation that fails, return the SDK's tool error result (`isError` in the protocol representation). "Resource not found; check the identifier" and "Permission denied for this workspace" give a client useful direction. Keep raw exception details and secrets out of the result. Authentication failure, no matches, rate limits and partial results should remain distinguishable.

Honor the service's retry guidance within a deadline and attempt limit. A read timeout may be retryable; a creation timeout can leave an unknown effect. Propagate cancellation to owned requests and close owned resources. Return an operation identifier if the service needs later reconciliation.

## Select and preserve the transport

Stdio fits a locally spawned server. Keep human logs on stderr; even a startup banner on stdout can corrupt the protocol. Streamable HTTP fits hosted or multi-client servers when the project and SDK support it. Use the transport the client actually requires rather than changing an existing deployment because a sample prefers another one.

For HTTP, validate authentication audience and scope before dispatch, protect session state, and use the SDK's origin/DNS-rebinding protections. A local development bind should not accidentally expose the server on every network interface. Tool-supplied paths or URLs do not expand allowed filesystem roots or destinations.

## A useful development check

From a separate client, discover the tools, search a known fixture, follow a second page and use a returned ID in a detail request. Assert the actual expected objects and effects. Add failure cases at the changed boundary, then run the normal project checks. Tests with a synthetic service need no paid model calls and do not claim compatibility with an untested production endpoint.
