# MCP server development

Adapted from Anthropic's Apache-2.0 mcp-builder skill and implementation references; see [UPSTREAM.md](UPSTREAM.md).

## Design around a real task

A server is useful when its tools let a client accomplish the requested task. Inspect the selected language, installed MCP SDK/version, service API and existing server before choosing an implementation. Keep the project's stack. Read installed types and official documentation for the relevant version; the bundled examples are patterns, not a guarantee about every SDK release.

Balance focused workflow tools with composable service operations. Expose the operations the task needs; do not mirror an entire API by default. Use descriptive, action-oriented names with a service prefix where collisions are likely, such as `issues_search` or `issues_create`. Descriptions should say what a tool does and when to use it.

For a new server or tool surface, read [references/server-patterns.md](references/server-patterns.md), which includes input/output shapes, pagination, transport and error patterns. The optional [assets/paginated_result.py](assets/paginated_result.py) ports the upstream pagination response into a dependency-free bounded helper; use it only when the target project uses Python and offset pagination.

## Implement the service boundary

Define required and optional fields, enum values, string lengths, numeric bounds and an output shape. Validate tool arguments before contacting the service. Keep list results focused and paginated; return continuation metadata and make truncation explicit. Do not collect an entire remote dataset merely to slice it locally.

Separate read operations from effects. Describe side effects and provide accurate MCP annotations, while enforcing authentication and authorization in the service boundary. An annotation is a hint, not a permission grant. Keep credentials out of tool arguments, results and diagnostics. Validate URLs, paths and resource identities against the caller's actual scope.

Use the installed SDK's server registration and transport helpers. On stdio, reserve stdout for protocol frames and write diagnostics to stderr. For HTTP, apply the SDK's supported authentication, session and origin handling; bind local development services narrowly. Propagate cancellation and deadlines into dependent work. A timed-out mutation may have completed: reconcile through its operation identity or idempotency contract before retrying.

Return tool execution failures as actionable tool results using the SDK's convention, while preserving protocol errors for malformed protocol requests. Explain a safe next step without exposing raw upstream exceptions or credentials.

## Try the workflow from a client

Use a local fixture or an already authorized service to exercise discovery and a useful tool sequence. Check returned data and actual effects. Target changed boundaries: invalid inputs, empty and multiple pages, denied operations, oversized responses or cancellation as relevant. Startup success alone does not show that a tool works.

Run the project's build/tests and available MCP client or inspector. Do not introduce an LLM evaluation harness for a normal server change. Report what worked, the SDK/transport used and any unavailable integration check. Registering the server, granting access, installing dependencies or publishing requires the corresponding existing authorization; loading this skill supplies none of those actions by itself.
