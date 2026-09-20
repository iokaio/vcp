# P7-03 canonical remote MCP tools

This increment connects the [HTTP boundary prerequisites](p7-mcp-http-boundaries.md)
to canonical tool discovery and execution. It follows
[ADR-027](../adr/027-owned-http-send-boundary.md). Resources and prompts remain
separate P7-03 work. The [qualification report](../evaluations/p7-03-http.md)
records the campaign scope and final evidence status.

## Configuration and authority

Coding profiles accept an optional `mcp_http` array alongside the stdio `mcp` array.
Names are unique across both transports, with at most sixteen registrations.
Each entry names an exact canonical HTTPS endpoint, allowed tools, explicit
limits and an optional credential reference:

```json
{
  "name": "remote",
  "endpoint": "https://example.test/mcp",
  "credential": {
    "reference": "remote-key",
    "environment": "VCP_MCP_REMOTE_TOKEN"
  },
  "allowed_tools": ["echo"],
  "limits": {
    "frame_bytes": 4096,
    "total_discovery_bytes": 8192,
    "tools": 8,
    "pages": 2,
    "timeout_ms": 10000,
    "stderr_bytes": 0
  }
}
```

The CLI reads only the named environment variable during owner setup and injects
its bearer value into the scoped in-memory resolver. Configuration has no token
field or arbitrary header map. URL userinfo, queries and fragments are rejected;
complete operations containing known credential or session values are rejected
before prepared-operation capture, including values in endpoint paths. Anonymous servers omit
`credential`. Credential expiry uses the existing coding profile deadline;
there is no refresh or automatic renewal. The host derives the current workspace
and owner authority when installing a credential. Configuration alone grants no
network permission.

Remote operations use the same `/mcp list`, `/mcp call` and `/mcp disconnect` controls
and model wrapper as stdio. A discovered tool identity binds its schema, catalog,
connection and resolved profile, including the trust snapshot. Server descriptions
and results remain external content. A logical HTTP session is not a running
process lease and does not use the stdio daemon approval-resume exception.

## Owned exchanges

An admitted operation owns a scheduler claim, runtime permit, durable intent,
source provenance and deadline. DNS resolution follows admission and intent;
one resolved address is pinned, with no address fallback. OS resolver work may
outlive cancellation, so the physical socket fence is not claimed to fence every
DNS packet. Source authority is checked before payload delivery, while owner,
generation, holds and credentials remain gated beneath TLS on every socket poll.

Each POST owns a fresh HTTP/1 connection and one attempt. A private exchange
observation binds the exact request body to its local TLS flush; response headers
alone cannot acknowledge a written request. Initialized notifications and control
responses additionally require an empty 202 before the next protocol step.
The adapter parks an SSE stream while a separately recorded callback response is
sent under the parent claim and deadline. It supports ping and rejects unsupported
server requests; it never delegates tool authority to callback content.

Only complete, validated protocol frames establish receipts. Cancellation or a
lost reply after possible delivery remains unknown and is never automatically
replayed. A validated reply remains evidence even if a later stream suffix fails.
Session expiry requires a fresh explicit initialization; SSE IDs and retry fields
cannot initiate reconnect or replay. Authentication and session headers remain
private, and known sensitive values must be screened before captures and outputs.

## Windows trust snapshot

Production registrations snapshot the current-user and local-machine `Root` and
`Disallowed` stores. The implementation opens fixed system stores with
`CERT_STORE_READONLY_FLAG` and `CERT_STORE_OPEN_EXISTING_FLAG`; these flags prevent
creation and persistence of changes. See Microsoft's
[CertOpenStore contract](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certopenstore).
Enumeration retains explicit context ownership and treats only the documented
end-of-enumeration result as completion; see
[CertEnumCertificatesInStore](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certenumcertificatesinstore).

Bounded snapshots include time-valid roots usable for server authentication,
respecting the special zero-usage results described by
[CertGetEnhancedKeyUsage](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certgetenhancedkeyusage).
Explicitly distrusted roots are removed; exact distrusted leaf and intermediate
certificates are rejected before normal WebPKI verification. Chain, name and
signature verification remain enabled. This is not an implementation of the full
Windows chain engine, CTL policy or online revocation checking.

Snapshots are immutable for the registration and participate in its digest.
Ambient `SSL_CERT_FILE` and `SSL_CERT_DIR` overrides are not consulted. Tests use
explicit synthetic roots without modifying OS trust stores. No new external
dependency version is introduced; the selected-source patch records the direct
edge to the already locked Windows API package.
