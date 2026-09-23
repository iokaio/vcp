# ADR-043 — Public protocol schemas and reconnect identity

Date: September 23, 2026.
Status: selected for P9-01; transport authentication remains P9-02.

## Decision

Use separate Rust public DTOs in `vcp-protocol` as the canonical v1.0 definitions.
Reuse the retained workspace's schemars 0.8.22 toolchain choice, behind an optional
`schema` feature. Export JSON Schema draft-07, then generate TypeScript using the
repository's bounded, dependency-free translator. Keep generated definitions in
`src/packages/protocol-ts`, separate from the subsequent hand-written SDK.

The exporter embeds hashes of its compiled canonical inputs. Routine checks
compare those hashes with the checkout and check all generated outputs; changing
a Rust definition requires actual native regeneration, not refreshing a sidecar
timestamp. Native tests and independent schema fixtures check semantic agreement.
The single additional lockfile dependency edge is recorded as Codex patch 37;
no dependency version changes.

The [protocol reference](../development/public-protocol.md) records the method
inventory, limits, evolution policy, generator commands and implemented adapter
subset. Schema registration never implies runtime capability or authorization.
Unimplemented methods must remain absent from negotiated server capabilities.

## Wire profile

Retain JSON-RPC 2.0, including its `jsonrpc` member, standard errors and request
correlation. Use bounded UTF-8 lines; reject invalid encoding, duplicate JSON keys,
oversized frames and incomplete EOF deterministically. Framing failure closes the
decoder. A literal null request ID differs from an absent ID. Integer transport
IDs are restricted to JavaScript's exact range; string IDs are recommended.

Unknown envelope fields and request governance fields fail closed. Successful
initialization negotiates a supported major/minor profile and explicit required
versus optional capabilities. Version alone conveys no feature or authority.
Clients must not map unknown state/outcome enums to success.

Engine operations require requests with IDs. Valid notifications produce no
response and no operation, including initialize and mutation notifications.
Process valid bounded batch entries in order, preserve each response ID and
suppress notification responses. Exceeding the 64-entry batch bound closes the
session before executing any entry. If a response cannot fit the negotiated
frame bound, close rather than inventing a differently correlated response.
A previously committed command remains discoverable through its durable ID.

Public counters and money use canonical unsigned decimal strings, including the
full u64 range. Schemas preserve scalar and collection limits; runtime validation
also enforces UTF-8 byte bounds, content hashes and state-dependent requirements.
TypeScript types do not replace runtime validation.

## Durable identity and shared adapters

Keep transport IDs separate from durable command IDs. Public command digests bind
the protocol version, authenticated actor, method, scope, revision preconditions
and semantic parameters. They exclude the transient controller/connection epoch.
Current access is checked before persisted receipt lookup. Same-key/same-payload
retries return the prior receipt across engine restart; changed payloads conflict.
Current observers cannot replay a command as a mutation.

Preserve the existing internal command digest unchanged. The public adapter alone
selects the domain-separated digest through a crate-private handler entry point;
the wire never supplies a trusted digest, actor, owner epoch or host facts.
Both paths commit through the same canonical store and command handler.

Extract bounded authenticated queries below the CLI instead of parsing CLI output
or exposing raw store maps. Existing grants are session-scoped; session listing
therefore exposes only the authorized session. Reading a receipt requires retained
scope evidence; missing evidence is unavailable rather than implicitly global.

## Alternatives and boundaries

Importing the entire Codex app-server protocol would omit VCP's required envelope
member and introduce unrelated task/permission semantics. Its schema separation
is reused deliberately without copying that wire contract. Adding another schema
language or generator dependency is unnecessary for this bounded DTO family.

ACP remains a possible future adapter, not the canonical task/budget/governance
contract. Local endpoint authentication, controller leases, live ownership,
bounded event delivery and compiled-server SDK qualification remain P9-02/03
work. This record neither advertises an authenticated server nor changes the
close-to-pause policy.
