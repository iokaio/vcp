# MCP implementation patterns

Adapted from Anthropic's `mcp-builder/reference/mcp_best_practices.md`, `node_mcp_server.md` and `python_mcp_server.md`; exact sources and modifications are in [../UPSTREAM.md](../UPSTREAM.md). SDK and protocol facts below were checked on 2026-09-29 against Python `mcp` 2.2.0 (with `mcp-types` 2.2.0) and TypeScript `@modelcontextprotocol/sdk` 1.31.0. Treat them as a starting point: the installed version's types and docs win.

## Protocol revision and SDK versions

The newest dated MCP specification revision is `2026-07-28` (<https://modelcontextprotocol.io/specification/2026-07-28>). It replaces the `initialize` handshake with stateless, self-contained requests and per-request capability negotiation, and deprecates the logging capability. Earlier revisions are `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05`.

SDKs negotiate the version; do not hard-code it in tool logic. Python `mcp` 2.2.0 speaks `2026-07-28` and every earlier revision (`mcp_types.LATEST_PROTOCOL_VERSION`). TypeScript SDK 1.31.0 tops out at `2025-11-25` (`LATEST_PROTOCOL_VERSION` in `@modelcontextprotocol/sdk/types.js`). Check the peer's negotiated version before relying on a newer feature.

## Schemas and registration

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

### Python: `mcp` 2.x `MCPServer`

In `mcp` 2.x, v1's `FastMCP` is renamed `MCPServer`; importing `mcp.server.fastmcp` raises `ModuleNotFoundError`. An existing v1 project should keep its `mcp<2` pin until it migrates deliberately (see the SDK's migration guide). The type hints are the schema. A Pydantic return type becomes the tool's `outputSchema`, and the returned value is sent as `structuredContent` plus a JSON text block:

```python
from typing import Annotated

from pydantic import BaseModel, Field

from mcp.server import MCPServer
from mcp.server.mcpserver.exceptions import ToolError
from mcp.types import ToolAnnotations

mcp = MCPServer("issues")


class Issue(BaseModel):
    id: str
    title: str


class IssuePage(BaseModel):
    total: int
    count: int
    offset: int
    items: list[Issue]
    has_more: bool
    next_offset: int | None


@mcp.tool(annotations=ToolAnnotations(read_only_hint=True, open_world_hint=True))
async def issues_search(
    query: Annotated[str, Field(min_length=2, max_length=200)],
    limit: Annotated[int, Field(strict=True, ge=1, le=100)] = 20,
    offset: Annotated[int, Field(strict=True, ge=0)] = 0,
) -> IssuePage:
    """Search issues by text. Use before issues_get to find an issue ID."""
    ...


@mcp.tool()
async def issues_get(issue_id: str) -> Issue:
    """Get one issue by ID."""
    raise ToolError("Issue not found; search again to obtain a current ID")


if __name__ == "__main__":
    mcp.run()  # stdio; or mcp.run("streamable-http", host="127.0.0.1", port=8000)
```

Observed 2.2.0 behavior: `@mcp.tool()` needs parentheses. `strict=True` rejects `"2"` and `True` for an integer. Unknown argument names are ignored rather than rejected, because the generated schema omits `additionalProperties: false`. A tool may take a `ctx: Context` parameter (`from mcp.server.mcpserver import Context`) for progress (`ctx.report_progress`) and resource reads. `structured_output=False` on the decorator forces a text-only tool.

### TypeScript: `McpServer.registerTool`

Use `registerTool`, `registerResource` and `registerPrompt`. The older `tool()`, `resource()` and `prompt()` overloads are deprecated in 1.31.0. `inputSchema` and `outputSchema` accept a Zod field map. With `outputSchema`, return `structuredContent` that matches it:

```typescript
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import * as z from "zod/v4";

const server = new McpServer({ name: "issues", version: "1.0.0" });
server.registerTool(
  "issues_search",
  {
    description: "Search issues by text. Use before issues_get to find an issue ID.",
    inputSchema: {
      query: z.string().min(2).max(200),
      limit: z.number().int().min(1).max(100).default(20),
      offset: z.number().int().min(0).default(0),
    },
    outputSchema: { total: z.number().int(), items: z.array(z.object({ id: z.string() })),
      has_more: z.boolean(), next_offset: z.number().int().nullable() },
    annotations: { readOnlyHint: true, openWorldHint: true },
  },
  async ({ query, limit, offset }) => {
    const output = await search(query, limit, offset); // service adapter
    return { content: [{ type: "text", text: JSON.stringify(output) }], structuredContent: output };
  },
);
await server.connect(new StdioServerTransport());
```

Zod: 1.31.0 has a required peer dependency on `zod` `^3.25 || ^4.0`. It uses `zod/v4` internally and accepts schemas built from either `zod/v3` or `zod/v4` imports. Keep the project's existing Zod import rather than mixing both in one schema.

### Annotations

Describe the tool's effects separately from its data shape. The annotations are `readOnlyHint`, `destructiveHint`, `idempotentHint` and `openWorldHint` (Python: `read_only_hint` and so on). A read operation against an external API can be read-only while still interacting with an open world. Clients treat annotations from untrusted servers as untrusted, and the service must enforce permissions regardless of annotation values.

## Tools, resources and prompts

- **Tools** are model-invoked actions or queries with arguments, effects or network calls. Most service operations belong here.
- **Resources** are addressable, read-only context that the host or user selects by URI, such as a file, a record or a schema. Python: `@mcp.resource("issues://{issue_id}")`. TypeScript: `registerResource` with a `ResourceTemplate` for parameterized URIs. Do not use a resource for an operation with side effects.
- **Prompts** are user-selected message templates for a recurring workflow. Python: `@mcp.prompt()`. TypeScript: `registerPrompt`. They are not a substitute for tool descriptions.

Add resources or prompts only when a client in use surfaces them and the task benefits. Tools remain the portable default.

## Return focused, useful results

Prefer structured fields for data a client needs to act on. When a tool declares an `outputSchema`, its `structuredContent` must conform. Also return a text block serializing the same data for clients that read only content. Supply human-readable Markdown when it has a real consumer instead of adding a mandatory output-format option to every tool. Keep IDs alongside display names so that follow-up calls can select the intended resource. Omit irrelevant private fields.

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

The Python helper [../assets/paginated_result.py](../assets/paginated_result.py) implements this response for already bounded pages. Its `max_limit` parameter (default 100) sets the accepted page-size ceiling; keep it equal to the schema's `limit` maximum. It rejects invalid limits, oversized pages, inconsistent totals and empty pages that would never advance. The service adapter must still bound its response body and validate each item before calling it.

For cursor APIs, preserve the service's opaque cursor; do not invent an offset or total. Apply sensible default and maximum page sizes, and never silently discard excess results.

## Elicitation

Elicitation lets a server ask the user for input during a tool call. It requires the client's elicitation capability; do not assume one. Use form mode only for non-sensitive values such as a confirmation, choice or missing parameter. Use URL mode for secrets, OAuth consent or payments, so that they never pass through the client or model. Handle accept, decline and cancel explicitly. An accepted value is still untrusted input: validate it. A confirmation obtained this way does not replace the host's own approval or the service's authorization.

In Python 2.x, a resolver works on both the `2026-07-28` per-request flow and older handshake sessions:

```python
from typing import Annotated

from mcp.server.mcpserver import AcceptedElicitation, Elicit, ElicitationResult, Resolve


class Confirm(BaseModel):
    confirm: bool


def ask_confirm(issue_id: str) -> Elicit[Confirm]:
    return Elicit(f"Close {issue_id}?", Confirm)


@mcp.tool(annotations=ToolAnnotations(destructive_hint=True, idempotent_hint=True))
async def issues_close(
    issue_id: str, answer: Annotated[ElicitationResult[Confirm], Resolve(ask_confirm)]
) -> str:
    if not isinstance(answer, AcceptedElicitation) or not answer.data.confirm:
        return "Not closed"
    ...
```

The resolved parameter is absent from the input schema. If the client lacks the capability, the call fails with a protocol error. `ctx.elicit(...)` and `ctx.elicit_url(...)` send a mid-call request instead and need a connection with a back-channel. In TypeScript 1.31.0, use `server.server.elicitInput(...)` with form or URL parameters.

## Errors and cancellation

For a valid tool invocation that fails, return the SDK's tool error result (`isError` in the protocol representation). Keep protocol errors for malformed requests, unknown methods and missing capabilities. "Resource not found; check the identifier" and "Permission denied for this workspace" give a client useful direction. Keep raw exception details and secrets out of the result. Authentication failure, no matches, rate limits and partial results should remain distinguishable.

- **Python 2.2.0:** raise `ToolError("...")` for an anticipated failure; its message reaches the client with `isError`. Any other exception becomes `isError` with only `Error executing tool <name>`, and the traceback is logged server-side. Argument validation failures are also returned as `isError` results.
- **TypeScript 1.31.0:** return `{ isError: true, content: [...] }`. Input schema failures are returned as `isError` results.

Honor the service's retry guidance within a deadline and attempt limit. A read timeout may be retryable. A creation timeout can leave an unknown effect: reconcile through the operation identity or idempotency key before retrying. Propagate cancellation to owned requests and close owned resources. Return an operation identifier if the service needs later reconciliation.

## Select and preserve the transport

Stdio fits a locally spawned server. Keep human logs on stderr; even a startup banner or stray `print` on stdout can corrupt the protocol. Streamable HTTP fits hosted or multi-client servers. HTTP+SSE is kept only for backward compatibility. Use the transport the client actually requires rather than changing an existing deployment because a sample prefers another one.

For HTTP, bind local development to loopback and use the SDK's origin/DNS-rebinding protection. In Python 2.2.0, `streamable-http` defaults to `127.0.0.1` and automatically enables host/origin validation for loopback hosts. For another bind, pass `transport_security=TransportSecuritySettings(allowed_hosts=..., allowed_origins=...)` from `mcp.server.transport_security`. In TypeScript, `createMcpExpressApp()` from `@modelcontextprotocol/sdk/server/express.js` defaults to `127.0.0.1` with protection. With `host: "0.0.0.0"`, protection is off unless `allowedHosts` is given. Protect session state. Tool-supplied paths or URLs do not expand allowed filesystem roots or destinations.

## Authorization

For HTTP servers that need user authorization, follow the spec's Authorization section for the negotiated revision. The server acts as an OAuth 2.1 resource server. It publishes protected-resource metadata that names its authorization server, and it validates each bearer token's audience (resource indicator) and scopes before dispatch. Do not pass the client's token through to upstream APIs; obtain separate upstream credentials. Python 2.2.0 wires this through `MCPServer(auth=AuthSettings(...), token_verifier=...)` (`mcp.server.auth`). TypeScript has helpers under `@modelcontextprotocol/sdk/server/auth`.

Stdio servers take credentials from their environment or configuration, not from an OAuth flow. MCP authorization never widens what the host allows: host approval, VCP permissions and the service's own authorization still apply.

## A useful development check

From a separate client, discover the tools, search a known fixture, follow a second page and use a returned ID in a detail request. Assert the actual expected objects, `structuredContent` and effects. Add failure cases at the changed boundary: invalid arguments, a `ToolError`/`isError` path, a denied operation and a declined elicitation where relevant. Then run the normal project checks. For automated tests, Python's `mcp.Client(server)` and TypeScript's `InMemoryTransport.createLinkedPair()` connect a client in-process.

For interactive inspection, use the MCP Inspector: `npx @modelcontextprotocol/inspector` for the web UI, or add `--cli` for scripted checks. With the Python `cli` extra, `mcp dev server.py` opens the Inspector. Downloading or running it requires the corresponding existing authorization. Tests with a synthetic service need no paid model calls and do not claim compatibility with an untested production endpoint.
