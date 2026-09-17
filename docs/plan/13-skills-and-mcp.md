# 13 — Bundled development skills and MCP

Status: planned. Owns P7-01, P7-02 and P7-03. Skill discovery follows P2-01/P2-03; bundled skills also need verification, and MCP needs the execution broker. Hooks/configuration import remain in segment 19.

## Code organization

Under `vcp-extensions`, separate `skill_manifest`, `discovery`, `activation`, `catalog`, `mcp/client`, `mcp/server_registry`, `mcp/schema`, `mcp/auth` and `mcp/invocation`. Package built-in skill bodies/assets under a proposed `src/skills/builtin/` directory with a versioned catalog. Keep generic skills as content plus metadata; they do not become a second tool executor.

`SkillDescriptor` carries ID/version/source/license, short description, activation cues, applicable environments, required tools and body hash. `ActivatedSkill` records the selected version and reason in the context manifest. `McpToolIdentity` binds server identity, connection generation, tool name and schema revision; it cannot be replaced by a plain display name.

Reuse qualified Gemini G06 discovery/lifecycle code and Pi-style lazy discovery patterns; inspect/pin exact source before copying. Keep transport/provider SDK details behind adapters.

Implement against [the extension design](../architecture/routing-extensions-design.md#skill-discovery-and-activation),
[extension-scope ADR-011](../adr/011-extension-scope.md) and
[compatibility ADR-014](../adr/014-foreign-compatibility.md). The latter is a decision
record for qualification, not a declaration that arbitrary foreign bundles already
work. Use the P0 source map and existing broker/context/artifact services; skill
content never becomes an alternative executor or trusted policy source.

## P7-01 — Discovery and activation

1. Discover configured and built-in skill descriptors with bounded traversal, deterministic identity resolution and explicit precedence. Load short descriptions first; read bodies only on activation.
2. Record scope/version/activation reason and honor current user and AGENTS.md instructions. Project skills cannot grant process/network authority or override trusted denials.
3. Invalidate affected manifests when a skill changes. Detect duplicate/conflicting IDs, malformed metadata, oversized content and out-of-scope paths with actionable errors.

Test small/large catalogs, lazy body-load counts, matching versus nonmatching projects, duplicate IDs, modified skill mid-task and malicious instructions. Measure discovery/schema context cost under E02/E03/U08 without hiding necessary capabilities.

**Construction sequence.** Define a source registry containing built-in,
user-configured and workspace sources, source trust/scope, bounded traversal rules
and revision. Parse only descriptors during discovery. Normalize stable component
identity independently of display name; order results deterministically and record
source precedence. Ambiguous duplicates at the same precedence produce a diagnostic,
not filesystem-order selection. Explicit selection may use a qualified identity.

Validate body/resource paths relative to the package root, then enforce canonical
containment and the repository link/reparse-point policy. Do not run executable
configuration, resolve arbitrary network references or traverse an entire home
directory to discover skills. Cache by source and descriptor revision; tests count
actual file reads to show that bodies were not eagerly loaded. Descriptor limits
must bound count, bytes and malformed-input cost as well as token usage.

Activation loads the pinned body/resources, checks their hashes and current scope,
and emits an event with reason, version, source and artifact references. Pass the
body through [instruction precedence](../architecture/vcp-what.md#62-instruction-discovery-and-precedence)
and [context skill selection](../architecture/vcp-what.md#64-tool-schemas-and-skills).
An explicit but missing/incompatible skill produces a visible setup diagnostic.
Changed skill content invalidates dependent future manifests and prepared work;
historical context retains its original attribution. Disable/uninstall prevents
new activation while the controller reconciles active effectful work normally.

Deliver descriptor/activation fixtures for source shadowing, resource path escape,
same-ID different content, stale caches, disabled running components and a skill
asking to override a trusted denial. Assert both context ordering and absence of
actual unauthorized broker dispatch; a well-formatted activation event alone is
insufficient evidence.

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

**Construction sequence.** Start each family from a coverage row and fixture,
then write a short descriptor and on-demand procedure. The procedure first reads
actual manifests, directory guidance, lockfiles and available toolchains; it then
chooses a bounded analysis/review/generation/check path. Specify expected evidence,
safe missing-prerequisite behavior and conditions for asking a material question.
Avoid unconditional dependency installation, full-suite execution or a prescribed
architecture that overrides the repository being edited.

Maintain a catalog manifest with component version, source/license, body/resource
hashes, activation cues and supported host/toolchain claims. Package those exact
assets and verify archive contents against the manifest during distribution.
External examples or adapted skill text need provenance and applicable notices;
new original content does not justify copying another catalog wholesale. Distinguish
analyze/review/generate support from actual execution support, especially for
toolchains not qualified on native Windows.

For each family provide a representative normal fixture plus one misleading cue
or missing-tool case. For example, a JavaScript directory with multiple lockfiles
must use project guidance or report ambiguity; a .NET fixture selects the declared
solution/target; a SQL migration fixture never treats sample connection settings as
permission to contact a database. Define assertions over discovered commands,
preserved files, cited evidence, seeded findings and reported not-run checks.
Preserve the full breadth in
[architecture section 15.6](../architecture/vcp-what.md#156-bundled-development-skills)
without claiming exhaustive language/framework coverage.

Deliver the coverage matrix with fixture IDs and independently recorded results.
Deterministic contract cases prove discovery and authority; live owner cases grade
usefulness under an explicit spend cap. A passing fake model cannot establish skill
quality, and unavailable host toolchains remain unvalidated rather than silently
excluded from the matrix.

## P7-03 — MCP lifecycle and tool calls

1. Implement explicit local/remote server configuration, identity, startup/connect/disconnect, bounded discovery and credentials via scoped references. A configured remote MCP service does not become a hosted VCP memory backend.
2. Normalize schema/content/errors into VCP tool types. Validate complete arguments, immutable schema identity and effective policy before dispatch; use the same durable intent/effect receipts as native tools.
3. Handle schema drift, reconnect, timeouts, output/resource limits and cancellation. A changed schema invalidates prepared calls and stale approvals. A disconnected non-idempotent remote operation remains outcome-unknown until reconciled.
4. Keep server prompts/resources attributed as external content. Request/response capture excludes auth secrets and respects workspace data policy; returned text cannot authorize another call.

Use controlled MCP fixture servers for echo/read/write marker, delayed result, schema-change-on-reconnect, malformed payload, tool-name collision, auth failure, large output and cancellation loss. Assert actual effect count, correct server/schema attribution and no authority bypass. Preserve uncertain outcomes on process restart.

**Construction sequence.** Choose and record a supported specification revision,
transport subset and tested adapter/library revisions. Implement registration,
connection/negotiation, bounded discovery, preparation, dispatch and reconciliation
as separate stages following
[MCP identity and dispatch](../architecture/routing-extensions-design.md#mcp-identity-and-dispatch).
Registrations bind stable server identity, configured endpoint/executable, scope,
auth references and capabilities. Broker local startup and enforce configured
network/data authority for remote endpoints. Resolve credentials only inside the
adapter; sanitization must cover errors as well as normal request/response capture.

Discovery normalizes schemas into VCP types while retaining source/schema hashes.
Use registration, connection generation, tool name and schema digest as identity;
never dispatch by an ambiguous display name. Treat tool annotations as hints and
derive effect authority from trusted VCP rules. Complete argument validation precedes
preparation. Before dispatch, recheck current policy, steering, connection generation
and schema identity; persist intent through the same tool lifecycle as built-ins.
Bind content-bearing arguments to source/context provenance and access/deletion
revisions, then recheck them at the common dispatch fence. A revoked or pruned source
invalidates a prepared upload and its cached arguments; test that no remote payload
is sent when deletion occurs after preparation but before dispatch.

Negotiate unsupported optional server-initiated capabilities as unavailable. In
particular, a library's model-assistance callback cannot bypass OpenRouter, root
budget admission or attribution; either integrate it through those boundaries or
disable that capability explicitly. Returned prompts/resources remain external
data. Resource reads and cache hits enforce current workspace scope, bounded sizes
and URI handling; returned URLs do not trigger automatic network/file access.

Reconnect rediscovers and compares identity/schema; changed schemas invalidate
prepared calls and approvals. Timeout/cancellation closes local waiting but is not
proof of absent remote effects. Use qualified remote status/receipt facilities when
available; otherwise retain unknown outcome and expose reconciliation options.
Do not claim a transport ID provides remote exactly-once semantics. Persist late
results to their original operation and prevent dispatch under a stale generation.

Add fault barriers after durable intent, after the fixture's marker write and before
local receipt commit. Restart the engine and assert one marker, retained unknown
state when evidence is unavailable, and no repeat call used as a status probe.
Exercise secret redaction, denied server startup, inaccessible resource cache,
unexpected server callbacks and shutdown with active calls. Acceptance combines
[architecture section 15.4](../architecture/vcp-what.md#154-mcp-lifecycle) with
[tool dispatch](../architecture/vcp-what.md#92-tool-dispatch-sequence) and common
recovery contracts; successful echo traffic is only the simplest fixture.

## Exit

Run `extensions`, applicable `context`, `tools`, E02/E03/E16/R06/U08 and catalog quality fixtures. Done when bundled skills activate visibly across the declared ecosystem breadth and configured MCP tools follow the same authority/recovery model as built-in tools. Unimplemented hooks/importers are not hidden prerequisites.
