# P7-03 resources, prompts and scoped cache

The original MCP 2025-11-25 codec now supports explicit resource and prompt
discovery over the existing governed [stdio](p7-mcp-stdio.md) and
[HTTPS](p7-mcp-http.md) adapters. The transport, approval, source-fence and
unknown-outcome contracts remain the same as tool dispatch.

## Configuration and controls

Both server configuration forms accept `allowed_resources` (exact resource URI
strings) and `allowed_prompts` (exact names), defaulting to empty sets. For example,
an existing registration can add:

```json
{
  "allowed_resources": ["notes://project/overview"],
  "allowed_prompts": ["review"]
}
```

Resource-only and prompt-only servers are supported. Configured capabilities must
be available on the initialized server. Discovery records bounded descriptors;
only explicitly allowed entries can be selected. The existing `limits.tools`
bound applies separately to each catalog; page, byte and deadline limits also
apply. No subscription, template expansion, completion, sampling or roots
capability is added.

The CLI exposes these commands alongside tool list/call/disconnect:

```text
/mcp resources SERVER
/mcp read SERVER URI IDENTITY_DIGEST
/mcp prompts SERVER
/mcp prompt SERVER NAME IDENTITY_DIGEST ARGUMENTS_JSON
/mcp cached SERVER ARTIFACT_ID
```

Selection uses the immutable identity returned by discovery. Prompt arguments
are a closed object of declared string fields, including required fields;
duplicate keys and undeclared arguments are rejected. Descriptor changes,
catalog invalidation and reconnect invalidate the previous selection. Requests
already prepared are checked again before send.

The model's existing five-string `vcp_mcp` interface uses actions `resources`,
`read_resource`, `prompts`, `get_prompt` and `read_cached`. Its `tool` selector
holds the URI, prompt name or artifact ID for the corresponding action. Unused
fields must be empty. These controls retain the calling model's source context
and pass through canonical admission and durable receipts.

## External content and URI handling

Resource URIs are bounded absolute ASCII URI strings, compared exactly without
normalization. Malformed escapes, whitespace and userinfo are rejected. A
`file:`, `https:` or custom URI is sent only to the configured MCP server; VCP
does not dereference it or open a local file. Returned links cannot start a
second request.

Resource text and prompt text/embedded text resources are normalized as external
data with source identity. Prompt message roles remain data and cannot become
system instructions or user authorization. Binary resources, image/audio blocks
and resource-link content are represented as explicit unsupported omissions.
The [exact numeric profile](p7-mcp-schema.md) also applies to metadata, including
fractional annotations; annotations remain inert. Text is never reparsed as
numeric JSON. Unsupported or malformed content cannot silently become a
successful complete text result.
Reserved serde_json internal object keys are rejected, including escaped
spellings, so dependency feature unification cannot reinterpret a decimal as an
admitted object or change a validated argument's type.

The pinned protocol references are the
[resource specification](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
and [prompt specification](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts).
The documented subset is deliberate; this is not a claim to implement every
optional MCP feature.

## Cache and recovery

A captured resource receipt can be read by its artifact ID as a prior
observation. The per-connection bounded cache index binds the artifact to its
resource identity and exact task/workspace scope. Reads recheck current Read
authority, artifact access and retention eligibility, current catalog and
registration identity, and the current credential revision for authenticated
HTTPS. The artifact read itself is bounded. Reconnect, invalidation, revocation
or pruning cannot turn a stale entry into a fresh server result.

Cache hits perform no server I/O, expose their original artifact as provenance,
and explicitly identify the result as prior external content. A miss or denial
does not automatically refresh. New server reads require normal admission.
Stdio cache access also requires its owned process to remain alive. Coordinated
stdio authority changes drain that process and invalidate its cache. Retention
compaction fences active access but preserves evidence bytes; actual purge still
requires the existing terminal-task and settled-effect protections.

Resource and prompt operations use the existing durable intent, receipt and
unknown-outcome machinery. A server may produce an effect even for a nominal
read; cancellation or lost replies therefore never authorize automatic replay.
Discovery receipts retain a null call identity, existing tool receipts retain
their direct tool identity, and content operations carry their typed content
identity. Prepared-operation evidence separately identifies the operation kind.
Known HTTP credential/session values are screened before captures and normalized
outputs using the existing secret-handling boundary.

Run `scripts/evals/mcp-content-qualification.ps1` under the native Windows build
environment for the content matrix and transport regressions. Evidence is
recorded in the [qualification report](../evaluations/p7-03-content.md).
