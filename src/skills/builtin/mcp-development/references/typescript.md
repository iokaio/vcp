# TypeScript MCP server patterns: `McpServer.registerTool`

Adapted from Anthropic's `mcp-builder/reference/node_mcp_server.md`; exact sources and modifications are in [../UPSTREAM.md](../UPSTREAM.md). SDK facts below were checked on 2026-09-29 against `@modelcontextprotocol/sdk` 1.31.0. Treat them as a starting point: the installed version's types and docs win. Language-independent schema, pagination, error, transport and authorization rules are in `references/protocol.md`.

## Version

TypeScript SDK 1.31.0 tops out at protocol revision `2025-11-25` (`LATEST_PROTOCOL_VERSION` in `@modelcontextprotocol/sdk/types.js`); the newest dated revision is `2026-07-28`. Let the SDK negotiate the version and check the peer's negotiated version before relying on a newer feature.

## Registration, output schemas and structured content

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

Annotations are `readOnlyHint`, `destructiveHint`, `idempotentHint` and `openWorldHint`.

## Resources and prompts

Resources: `registerResource`, with a `ResourceTemplate` for parameterized URIs. Prompts: `registerPrompt`. Add them only when a client in use surfaces them.

## Elicitation

In TypeScript 1.31.0, use `server.server.elicitInput(...)` with form or URL parameters. It requires the client's elicitation capability. Use URL mode for secrets, and validate an accepted value.

## Errors

Return `{ isError: true, content: [...] }` for a failed tool invocation. Input schema failures are returned as `isError` results.

## HTTP transport security

`createMcpExpressApp()` from `@modelcontextprotocol/sdk/server/express.js` defaults to `127.0.0.1` with origin/DNS-rebinding protection. With `host: "0.0.0.0"`, protection is off unless `allowedHosts` is given. On stdio, keep diagnostics on stderr; stdout carries protocol frames.

## Authorization

TypeScript has HTTP authorization helpers under `@modelcontextprotocol/sdk/server/auth`. Validate token audience and scopes as described in `references/protocol.md`; MCP authorization never widens host or VCP permissions.

## Testing

For automated tests, `InMemoryTransport.createLinkedPair()` connects a client in-process. For interactive inspection, `npx @modelcontextprotocol/inspector` opens the MCP Inspector (add `--cli` for scripted checks); downloading or running it requires the corresponding existing authorization.
