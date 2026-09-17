# ADR-011 — Instructions, bundled skills and MCP

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P7-01/02/03; later P10-01/02. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#15-instructions-skills-hooks-and-mcp) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

AGENTS.md, bundled development skills and configured MCP tools are first-release requirements. Executable hooks and foreign configuration imports remain deferred. Extensions cannot bypass engine authority or accounting.

## Implementation proposal

Discover bounded descriptors lazily; bind activated bodies and tool schemas to exact revisions in context. MCP identities include server, connection generation, tool name and schema revision. Validate complete arguments and scope, commit intent, then invoke through the common broker. Reconnect/schema drift invalidates prepared calls.

Detailed contracts and failure ordering are in the [supporting design](../architecture/routing-extensions-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Reuse qualified Gemini lifecycle seams and selected lazy-loading patterns. Pin protocol/spec and dependency versions during P7 implementation. Keep skill content distinct from executable extensions and server resources as untrusted evidence.

## Qualification evidence

E02/E03/E16/R06/U08 cover lazy-load counts, duplicate identities, hostile content, missing toolchains, auth failures, schema mutation and lost non-idempotent replies. Built-in catalog quality uses representative project conventions and declared host coverage.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Broad skill coverage increases validation work; descriptor presence is not proof of correct guidance. Remote MCP effects may remain unknown after cancellation and must not be retried blindly.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
