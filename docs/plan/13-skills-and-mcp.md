# 13 — Bundled development skills and MCP

Status: planned. Owns P7-01, P7-02 and P7-03. Skill discovery follows P2-01/P2-03; bundled skills also need verification, and MCP needs the execution broker. Hooks/configuration import remain in segment 19.

## Code organization

Under `vcp-extensions`, separate `skill_manifest`, `discovery`, `activation`, `catalog`, `mcp/client`, `mcp/server_registry`, `mcp/schema`, `mcp/auth` and `mcp/invocation`. Package built-in skill bodies/assets under a proposed `src/skills/builtin/` directory with a versioned catalog. Keep generic skills as content plus metadata; they do not become a second tool executor.

`SkillDescriptor` carries ID/version/source/license, short description, activation cues, applicable environments, required tools and body hash. `ActivatedSkill` records the selected version and reason in the context manifest. `McpToolIdentity` binds server identity, connection generation, tool name and schema revision; it cannot be replaced by a plain display name.

Reuse qualified Gemini G06 discovery/lifecycle code and Pi-style lazy discovery patterns; inspect/pin exact source before copying. Keep transport/provider SDK details behind adapters.

## P7-01 — Discovery and activation

1. Discover configured and built-in skill descriptors with bounded traversal, deterministic identity resolution and explicit precedence. Load short descriptions first; read bodies only on activation.
2. Record scope/version/activation reason and honor current user and AGENTS.md instructions. Project skills cannot grant process/network authority or override trusted denials.
3. Invalidate affected manifests when a skill changes. Detect duplicate/conflicting IDs, malformed metadata, oversized content and out-of-scope paths with actionable errors.

Test small/large catalogs, lazy body-load counts, matching versus nonmatching projects, duplicate IDs, modified skill mid-task and malicious instructions. Measure discovery/schema context cost under E02/E03/U08 without hiding necessary capabilities.

## P7-02 — Built-in skill catalog

Create each skill with bounded purpose, project-detection cues, analysis/review/generation workflow, tool-discovery/check guidance, expected outputs and explicit missing-tool behavior. Generic advice must adapt to existing architecture and lockfiles.

| Family | Proposed catalog grouping | Representative fixture |
|---|---|---|
| Architecture/organization | `architecture` | Layered modules with a forbidden dependency and established error/config conventions |
| Analysis/review/debugging | `review-debug` | Seeded correctness defect plus benign diff and reproducible failing check |
| Verification | `testing` | Multiple test targets; choose relevant checks and invalidate stale results |
| Git/change hygiene | `git-workflow` | Dirty staged/unstaged/untracked state and an isolated child worktree |
| JS/TS/web | `javascript-typescript` | Lockfile, workspace packages, framework conventions and project scripts |
| Python | `python` | pyproject/requirements with existing environment and test/lint configuration |
| Rust | `rust` | Cargo workspace, pinned toolchain and features |
| .NET/PowerShell | `dotnet-powershell` | Multi-project solution, framework targets and Windows quoting |
| Java/Kotlin | `jvm` | Maven/Gradle wrapper and module-specific checks |
| Go | `go` | Modules/workspaces and build tags |
| C/C++ | `cpp` | CMake/MSBuild configuration with known compiler/generator |
| Ruby/PHP/Swift/Dart | Separate ecosystem descriptors/bodies | Manifests and conventions; unsupported Windows checks reported honestly |
| Shell/SQL/data/infrastructure | Separate toolset descriptors/bodies | Migration, script, container or CI change with environment constraints |
| Optimization/memory hygiene | `project-optimize` and `memory-hygiene` | Evidence-backed routing diff or precise pruning preview |

Publish coverage as analyze/review/generate/test capabilities per family and validated host/toolchain. Package only versioned assets with source/license records. A skill must not install a missing toolchain merely because it recommends a check.

Tests pair descriptors with small source fixtures and deterministic expectation checks: correct project commands/paths, architecture conventions, lockfile preservation, useful findings and explicit not-run checks. Use selected live U01–U03/U08 tasks for output quality; do not grade by exact prose matching.

## P7-03 — MCP lifecycle and tool calls

1. Implement explicit local/remote server configuration, identity, startup/connect/disconnect, bounded discovery and credentials via scoped references. A configured remote MCP service does not become a hosted VCP memory backend.
2. Normalize schema/content/errors into VCP tool types. Validate complete arguments, immutable schema identity and effective policy before dispatch; use the same durable intent/effect receipts as native tools.
3. Handle schema drift, reconnect, timeouts, output/resource limits and cancellation. A changed schema invalidates prepared calls and stale approvals. A disconnected non-idempotent remote operation remains outcome-unknown until reconciled.
4. Keep server prompts/resources attributed as external content. Request/response capture excludes auth secrets and respects workspace data policy; returned text cannot authorize another call.

Use controlled MCP fixture servers for echo/read/write marker, delayed result, schema-change-on-reconnect, malformed payload, tool-name collision, auth failure, large output and cancellation loss. Assert actual effect count, correct server/schema attribution and no authority bypass. Preserve uncertain outcomes on process restart.

## Exit

Run `extensions`, applicable `context`, `tools`, E02/E03/E16/R06/U08 and catalog quality fixtures. Done when bundled skills activate visibly across the declared ecosystem breadth and configured MCP tools follow the same authority/recovery model as built-in tools. Unimplemented hooks/importers are not hidden prerequisites.
