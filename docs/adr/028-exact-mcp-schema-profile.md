# ADR-028 — Exact MCP schema profile

Status: implemented with [native qualification](../evaluations/p7-03-numeric.md).
Decision gate: P7-03, with P1-04 persistence and P6 decision-codec compatibility.
Supersedes only the integer/schema restriction in
[ADR-026](026-governed-mcp-stdio.md). Its transport, authority and recovery decisions
remain in effect.

## Context

The initial MCP profile rejected decimal/exponent values to prevent binary-float
rounding. That restriction also excluded useful schemas and fractional metadata.
Broader support must preserve the values admitted by schema validation through
actual dispatch, secret filtering, evidence capture and later model context.

The selected MCP revision remains
[2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
The implementation admits a documented subset of JSON Schema 2020-12, with
explicit resource limits; it does not advertise general JSON Schema compliance.

## Decision

Use an original bounded lexical JSON reader and decimal coefficient/exponent
arithmetic inside `vcp-extensions::mcp`. Retain the existing serde_json Value
representation with an explicit `arbitrary_precision` dependency feature. Do not
add another public JSON tree, a schema-engine dependency or a provider-specific
number representation.

`mcp-schema/2` normalizes mathematically equivalent numeric spellings, including
signed zero, before schema/argument hashing. It preserves exact values and never
uses an epsilon or f64 intermediate. Original input byte limits and a bounded
normalized output prevent exponent expansion from escaping the frame allowance.
Shared work and digit allowances bound parsing, graph traversal and predicates.

The [profile contract](../development/p7-mcp-schema.md) defines numeric bounds,
nullable fields, dictionaries, acyclic local definitions, composition and exact
equality. Unsupported keywords, remote reference semantics, decoded duplicate
keys and serde-private marker keys fail closed, including in unused schema
branches. Numeric metadata and annotations remain data, never authority.

Compile references into a bounded node graph without expanding shared subtrees.
Budget exhaustion is an error, including inside negation or alternatives; it must
not become a successful nonmatch. No schema processing opens files, resolves
URLs, runs patterns or calls a model.

## Identity and compatibility

Every new ConnectionIdentity serializes `admission_profile: "mcp-schema/2"`.
Schema digest input is `mcp-schema/2`, a NUL byte, then normalized schema JSON.
This separates interpretation from the old profile even when an integer-only
schema's JSON is unchanged. Tool/content identities inherit the connection pin;
reconnect still creates a fresh generation. Existing approvals and cache entries
cannot acquire the new semantics by retaining an old digest.

Canonical protocol v1 and historical bytes/hashes are unchanged. MCP normalization
is confined to newly admitted MCP JSON; text content remains opaque. The
[P1-04 repair](../development/p1-persisted-json.md) preserves historical literal
objects independently. The P6 decision codec separately preserves numeric Values
without MCP normalization, retaining existing probability and monetary rules.
No workspace-wide serde feature or external dependency version is changed.

## Qualification and reconsideration

Pure tests must establish exact predicates, bounded graph work and rejection
semantics. Native stdio/HTTPS fixtures must independently observe actual numeric
wire bytes, receipt/capture values and later model context on both storage engines.
Secret filtering, invalid output, loss after an independent effect and no replay
remain required. Record results in the
[qualification report](../evaluations/p7-03-numeric.md).

Reconsider additional keywords or a maintained schema library when a concrete
server requires them and the implementation can preserve deterministic resource
bounds, exact arithmetic and the same authority/identity boundary. A declared
schema dialect does not authorize silent approximation of unsupported features.
