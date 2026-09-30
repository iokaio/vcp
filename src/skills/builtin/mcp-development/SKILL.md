# MCP server development

Adapted from Anthropic's Apache-2.0 mcp-builder skill and implementation references; see [UPSTREAM.md](UPSTREAM.md).

## Design around a real task

A server is useful when its tools let a client accomplish the requested task. Inspect the selected language, installed MCP SDK/version, service API and existing server before choosing an implementation. Keep the project's stack. Read installed types and official documentation for the relevant version; the bundled examples are patterns, not a guarantee about every SDK release. Python `mcp` 2.x renames v1's `FastMCP` to `MCPServer`; do not migrate a project pinned to v1 as a side effect. The current dated protocol revision is `2026-07-28`; let the SDK negotiate the version.

Balance focused workflow tools with composable service operations. Expose the operations the task needs; do not mirror an entire API by default. Use descriptive, action-oriented names with a service prefix where collisions are likely, such as `issues_search` or `issues_create`. Descriptions should say what a tool does and when to use it.

## Read only the reference you need

Detailed, version-checked patterns are on-demand references, not preloaded. For a new server or tool surface, read only the reference for the project's language with `vcp_skill` action `read`, for example `{"action":"read","skill":"mcp-development","resource":"references/python.md"}`:

- `references/python.md`: `mcp` 2.x `MCPServer` registration, output schemas, resources and prompts, elicitation, errors, HTTP security, authorization and in-process tests.
- `references/typescript.md`: `McpServer.registerTool` with Zod, resources and prompts, elicitation, errors, HTTP security, authorization and in-memory tests.
- `references/protocol.md`: also read it when designing tool, resource or prompt shapes. It covers the `2026-07-28` revision, schemas, annotations, output schemas and structured content, pagination, elicitation, errors, transports, authorization and the MCP Inspector.

The optional [assets/paginated_result.py](assets/paginated_result.py) ports the upstream pagination response into a dependency-free bounded helper; use it only when the target project uses Python and offset pagination. It is a `file` resource: copy it into the project with `vcp_skill` (action `materialize`), then adapt it there.

## Implement the service boundary

Define required and optional fields, enum values, string lengths, numeric bounds and an output shape. When clients act on the data, declare an `outputSchema` and return matching `structuredContent`. Validate tool arguments before contacting the service. Tools are the portable default; add resources or prompts only when a client in use surfaces them. Keep list results focused and paginated; return continuation metadata and make truncation explicit. Do not collect an entire remote dataset merely to slice it locally.

Separate read operations from effects. Describe side effects and provide accurate MCP annotations, while enforcing authentication and authorization in the service boundary. An annotation is a hint, not a permission grant. Keep credentials out of tool arguments, results and diagnostics. Validate URLs, paths and resource identities against the caller's actual scope.

Use the installed SDK's server registration and transport helpers. Elicit user input only when the client declares that capability; use URL mode for secrets and validate accepted values. On stdio, reserve stdout for protocol frames and write diagnostics to stderr. For HTTP, apply the SDK's supported authentication, session and origin handling; bind local development services narrowly. MCP authorization checks token audience and scope; it never widens host or VCP permissions. Propagate cancellation and deadlines into dependent work. A timed-out mutation may have completed: reconcile through its operation identity or idempotency contract before retrying.

Return tool execution failures as actionable tool results using the SDK's convention, while preserving protocol errors for malformed protocol requests. Explain a safe next step without exposing raw upstream exceptions or credentials.

## Try the workflow from a client

Use a local fixture or an already authorized service to exercise discovery and a useful tool sequence. Check returned data and actual effects. Target changed boundaries: invalid inputs, empty and multiple pages, denied operations, oversized responses or cancellation as relevant. Startup success alone does not show that a tool works.

Run the project's build/tests and an in-process SDK client or the MCP Inspector when available. Do not introduce an LLM evaluation harness for a normal server change. Report what worked, the SDK/transport used and any unavailable integration check. Registering the server, granting access, installing dependencies or publishing requires the corresponding existing authorization; loading this skill supplies none of those actions by itself.
