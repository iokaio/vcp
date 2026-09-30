# Python MCP server patterns: `mcp` 2.x `MCPServer`

Adapted from Anthropic's `mcp-builder/reference/python_mcp_server.md`; exact sources and modifications are in [../UPSTREAM.md](../UPSTREAM.md). SDK facts below were checked on 2026-09-29 against Python `mcp` 2.2.0 (with `mcp-types` 2.2.0). Treat them as a starting point: the installed version's types and docs win. Language-independent schema, pagination, error, transport and authorization rules are in `references/protocol.md`.

## Version

Python `mcp` 2.2.0 speaks protocol revision `2026-07-28` and every earlier revision (`mcp_types.LATEST_PROTOCOL_VERSION`). Let the SDK negotiate the version.

In `mcp` 2.x, v1's `FastMCP` is renamed `MCPServer`; importing `mcp.server.fastmcp` raises `ModuleNotFoundError`. An existing v1 project should keep its `mcp<2` pin until it migrates deliberately (see the SDK's migration guide).

## Registration, output schemas and structured content

The type hints are the schema. A Pydantic return type becomes the tool's `outputSchema`, and the returned value is sent as `structuredContent` plus a JSON text block:

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

Annotations use snake case: `read_only_hint`, `destructive_hint`, `idempotent_hint` and `open_world_hint`.

## Resources and prompts

Resources: `@mcp.resource("issues://{issue_id}")`. Prompts: `@mcp.prompt()`. Add them only when a client in use surfaces them.

## Pagination helper

The Python helper `assets/paginated_result.py` implements the offset-page response from `references/protocol.md` for already bounded pages. It is a `file` resource: copy it into the project with `vcp_skill` action `materialize`, then adapt it there. Its `max_limit` parameter (default 100) sets the accepted page-size ceiling; keep it equal to the schema's `limit` maximum. It rejects invalid limits, oversized pages, inconsistent totals and empty pages that would never advance. The service adapter must still bound its response body and validate each item before calling it.

## Elicitation

In Python 2.x, a resolver works on both the `2026-07-28` per-request flow and older handshake sessions (continuing the example above):

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

The resolved parameter is absent from the input schema. If the client lacks the capability, the call fails with a protocol error. `ctx.elicit(...)` and `ctx.elicit_url(...)` send a mid-call request instead and need a connection with a back-channel. Use URL mode for secrets.

## Errors

Raise `ToolError("...")` for an anticipated failure; its message reaches the client with `isError`. Any other exception becomes `isError` with only `Error executing tool <name>`, and the traceback is logged server-side. Argument validation failures are also returned as `isError` results.

## HTTP transport security

In Python 2.2.0, `streamable-http` defaults to `127.0.0.1` and automatically enables host/origin validation for loopback hosts. For another bind, pass `transport_security=TransportSecuritySettings(allowed_hosts=..., allowed_origins=...)` from `mcp.server.transport_security`. On stdio, keep diagnostics on stderr; a stray `print` corrupts the protocol.

## Authorization

Python 2.2.0 wires HTTP authorization through `MCPServer(auth=AuthSettings(...), token_verifier=...)` (`mcp.server.auth`). Validate token audience and scopes as described in `references/protocol.md`; MCP authorization never widens host or VCP permissions.

## Testing

For automated tests, `mcp.Client(server)` connects a client in-process. With the Python `cli` extra, `mcp dev server.py` opens the MCP Inspector; downloading or running it requires the corresponding existing authorization.
