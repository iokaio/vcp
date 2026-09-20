# P1-04 literal JSON preservation

The P7-03 numeric audit reproduced a pre-existing P1-04 storage failure under the
current CLI dependency graph. With serde_json's `arbitrary_precision` and
`raw_value` features enabled, its generic Value deserializer can interpret a
literal object containing `$serde_json::private::Number` or
`$serde_json::private::RawValue` as internal serialization machinery. Valid
record/event data could commit, then change meaning during replay and fail the
durable receipt comparison on both storage engines.

These keys are ordinary data in persisted record/event values. MCP and model
admission may reject them under their own profiles, but that cannot repair older
stored observations or justify rewriting their hashes.

## Decode boundary

`vcp_protocol::persisted_json` captures JSON text and distinguishes objects,
arrays and scalar tokens before constructing a Value. Literal objects retain
their decoded keys. Genuine scalar numbers use the active serde numeric
representation without MCP normalization or an intermediate float conversion.
Decoded duplicate keys and structural/byte excess fail explicitly.

The field reader permits at most 16 MiB, 128 nested levels and a node allowance
bounded by input byte length. Existing store limits are tighter for admitted
records (1 MiB) and transactions including events (8 MiB). The per-field depth
ceiling does not reduce the old reader's 128-level limit including enclosing
typed structures. EventPage allows 128 MiB for the envelope, above the store's
64 MiB logical-state ceiling. These limits do not increase store admission caps.

`Record.value` and `EventInput.data` use this reader. `Mutation` and `EventPage`
decode their raw discriminator before selecting a typed payload, avoiding the
intermediate serde enum buffer that cannot carry RawValue. Mutation retains its
closed field contract; EventPage retains its existing additive-field behavior.

Canonical serialization v1, field names, record revisions, receipt digests,
journal framing and SQLite schema remain unchanged. Reopening supported history
uses the original bytes and validates the original receipt. There is no migration,
replacement digest, escaped-key wire format or relaxed integrity check.

## Scope

The fix covers the persisted Record/Event boundaries and their typed containing
envelopes. It does not replace serde_json globally. `Record::decode<T>()` remains
a conversion to a known typed contract using that type's Deserialize behavior.
Consumers needing the already retained arbitrary JSON tree use `Record.value`;
converting it again into generic Value is unnecessary and retains serde's private
marker interpretation. No production `Record::decode<Value>()` caller was found
in the targeted audit.

The protocol dependency explicitly requests `raw_value`; no external dependency
version changes. The actual CLI already enabled both relevant serde features
before this change. Default and arbitrary-precision package graphs need separate
qualification because their scalar numeric representations differ.

## Qualification

Run `scripts/evals/persisted-json-qualification.ps1` under the native Windows build
environment. The runner records source identities, exact commands and test/log
identities and rejects changed source or failed/empty test stages. The
[qualification report](../evaluations/p1-persisted-json.md) records completed
evidence and limits.
