# Governed MCP stdio development contract

This document describes the implemented stdio slice of P7-03. It does not record
qualification results or claim complete MCP support. See
[ADR-026](../adr/026-governed-mcp-stdio.md) and the
[duplex prerequisite](p7-mcp-duplex.md).

## Registration and use

The explicit user-owned VCP JSON profile accepts an optional `mcp` array. Each
entry contains `name`, a `process` request referencing an existing trusted process
profile, `allowed_tools`, and `limits`. Project content, server metadata and model
output cannot register servers or authorize executables. No servers ship enabled.

The process must be direct, non-terminal and have no prepared stdin. Its profile
supplies the pinned executable, working-root rules, filtered environment and
isolation requirements. Registration resolves a digest of that profile and exact
request; the host rechecks it at use. The process lifetime remains bounded to
120 seconds. An example fragment, to be added to an otherwise complete profile:

```json
{
  "mcp": [{
    "name": "local-helper",
    "process": {
      "profile": "trusted-helper",
      "arguments": [],
      "directory": "",
      "input": null,
      "timeout_ms": 120000,
      "output_bytes": 1048576
    },
    "allowed_tools": ["echo"],
    "limits": {
      "frame_bytes": 65536,
      "total_discovery_bytes": 262144,
      "tools": 32,
      "pages": 8,
      "timeout_ms": 10000,
      "stderr_bytes": 65536
    }
  }]
}
```

The process request shape is the existing `vcp_tools::process::Request`; the
configured profile must already exist and startup still requires effective policy.
This example contains no credentials. Authenticated transports and credential
injection/sanitization are not implemented by this slice; do not use arguments,
environment values or protocol content as an improvised credential channel.

Terminal controls are explicit:

```text
/mcp list local-helper
/mcp call local-helper echo <listed-identity-digest> {"text":"hello"}
/mcp disconnect local-helper
```

Listing starts the configured process through its existing startup approval path
and returns only current admitted tools, plus rejection diagnostics. Calls require
the listed identity digest and arguments accepted by that tool's schema. The
`vcp_mcp` model wrapper uses the same list/call/disconnect path and requires an
isolated model response. A connected arbitrary server retains the exclusive
process claim; disconnect before native tools or verification.
Only one such opaque process can hold that claim at a time. Starting another
server returns a conflict immediately; it does not queue behind a connection that
the same caller would need to disconnect.

Startup and a later tool call can require separate explicit approvals. After an
exact pending call is approved, a task in `WaitingForInput` can deliberately resume
while its owned idle server remains connected. This uses a private in-memory
proof: the slot is locked against concurrent IO, the live process keeps its pins
and exclusive lease, and the pending call/approval, effect revision, connection,
owner, sources and current policy must match. A final lifecycle lock preserves
the owner/generation check through the resume transition.

Only that exact running process-lifetime effect is excluded from the resume
reconciliation check. An unrelated running effect, dispatch-recorded effect or
unknown outcome still blocks resume. This path cannot resume `Paused` tasks or
held owners, and it does not authorize a call: ordinary current call/source checks
run again before dispatch. The proof is not persisted or restored after reopen.

## Core protocol profile

`Client` owns no IO. It produces `Outbound` bytes and accepts one bounded complete
frame at a time. The host must validate the outbound token before sending,
revalidate canonical permission/source conditions, write once, then call
`confirm_sent`. Failure or cancellation abandons the connection. Matching content
from a different client cannot reuse a token, and confirming a frame twice fails.

Initialize requests and responses must use `2025-11-25`; the initialized notice
must be sent before discovery. The client supports one outstanding request and
requires the exact response ID. JSON-RPC batches, duplicate keys, unmatched
responses and malformed envelopes fail closed. Tools are collected privately
until all bounded pages complete; duplicate names and cursor cycles close the
connection. Unsupported schemas remain rejected candidates, not callable tools.

Ping can produce an empty control result. Other callbacks produce method-not-found
without running a model, exposing roots or acquiring credentials. Recognized
progress/log notifications do not extend deadlines. Tool-list changes invalidate
the catalog; host integration requires explicit reconnect and preparation. There
is no automatic replay, cache reuse or reconnect.

Tool results retain text blocks and optional object `structuredContent` as
untrusted data. Declared supported output schemas are checked. Image, audio,
embedded-resource and resource-link blocks produce explicit omission labels in
the normalized result; original bytes remain in host capture. URLs are not fetched.
Unknown content kinds fail the restricted protocol profile. Tool `isError` and
JSON-RPC errors remain distinct from successful results and transport uncertainty.

## Schema and numeric profile

Schemas must be objects with an explicit `type`; tool input/output roots must have
type `object`. The only accepted `$schema` value is
`https://json-schema.org/draft/2020-12/schema`. Omitting it is supported.

| Schema feature | Accepted behavior |
|---|---|
| Object | `properties`, unique string `required`, boolean `additionalProperties`; omission means true |
| Array | Uniform `items`, nonnegative integer `minItems` and `maxItems` |
| String | `minLength` and `maxLength` count Unicode scalar values |
| Primitive types | `string`, `integer`, `number`, `boolean`, `null`; no type unions |
| Scalar equality | Nonempty bounded `enum` or `const` containing string, boolean or null values |
| Inert schema annotations | String `title` and `description` |

Unknown keywords are rejected recursively. This includes references/definitions,
composition, conditionals, patterns, formats, numeric range/multiple constraints,
schema-valued additional properties and unsupported object/array applicators.
Numeric enum/const values are rejected rather than compared after float rounding.

All JSON numeric values in this initial wire/argument profile must deserialize
exactly as `i64` or `u64`. Decimal/exponent forms, including `1.0` and `1e0`, and
out-of-range integers are rejected, even inside otherwise permitted unknown
properties. This is an explicit interoperability restriction: JSON Schema itself
considers `1.0` an integer. Canonical argument bytes never silently round values.

The schema/argument default cap is 128 KiB, depth 16 and 4096 value nodes. The
bounded parser charges nodes and depth before parsing child values; a hard byte
cap bounds individual strings/keys. Protocol frames use configured byte limits
with fixed depth 32 and 65536 value nodes. Registration, total discovery, page,
tool, frame-count and process limits also apply. Native stdout/stderr remain under
the shared process output ceiling, and `stderr_bytes` additionally limits stderr.
The native reader retains each observed bounded read before checking the ceiling;
the crossing read can exceed the configured threshold before the job is stopped.

## Canonical provenance and outcomes

Tool identity includes registration digest, fresh connection UUID, remote name,
input schema digest and catalog revision. The checked argument object binds
canonical bytes to the schema digest. Model-produced requests also carry the
verified accounted context, artifact/file dependencies, memory send fence and
scope/authority/deletion/steering/binding/skill revisions. Owner-entered calls
record their explicit input provenance. Both preserve the startup process's
conservative Read/Write/Execute/Network/Install/Publish/Opaque effect set and
write-capable resources. Neither route obtains authority from server descriptions
or read-only/idempotence annotations.

The host prepares a separate call operation and durably records dispatch before
writing. Source/prune and authority checks run again at the send boundary. A
server that changes a marker and exits before returning a valid response has an
unknown outcome, not an automatically retriable failure. Closing a process is
also separate from success of any individual tool call.

Rejected or abandoned startup/call proposals are cancelled only while their
effects remain definitely unsent. Disconnect and owner shutdown also retire
pending proposals. Cancelling advances the effect revision, so any historical
pending question becomes non-actionable under the existing question-freshness
rules. It does not manufacture an approval decision. A durable dispatch intent,
running call or unknown outcome remains subject to its normal receipt/recovery
path.

Applying a history deletion or cleanup while an MCP connection exists, or its
slot is busy, synchronously holds the owner tree before the canonical mutation.
That hold advances the same admission generation checked under the lifecycle
mutex during every native write poll. A frame whose source check already passed
cannot start or continue writing under the old generation after deletion begins.
Owned interruption and process drain continue even when their waiter is dropped;
the canonical worker does not block waiting for those callbacks.

The host applies the exact selected prune preview before recording canonical task
pauses, so its own pause events do not invalidate the preview's source digest.
It then pauses nonterminal tasks, including when the apply attempt returns an
error. A stale or invalid preview is never silently regenerated. Inspect the
deletion result and any uncertain effect, refresh a stale preview if necessary,
and deliberately resume after interruption/reconciliation. MCP does not reconnect
or replay a call automatically. Startup retention runs before live connections
exist; offline retention still requires exclusive store ownership.

Native fixture tests must exercise actual pipe bytes, marker effect counts,
schema/identity changes, prune-before-send, callback rejection, malformed/oversize
output and interrupted receipts. Pure codec tests establish parser/state behavior;
they do not establish live server interoperability or model usefulness.
