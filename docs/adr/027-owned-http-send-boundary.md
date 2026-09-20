# ADR-027 — Owned HTTP send boundary

Status: accepted implementation direction for P7-03 transport prerequisites.
Canonical remote MCP integration and its acceptance gates remain separate.

## Decision

Use the existing locked Hyper HTTP/1 and Rustls libraries behind an original
owned transport adapter. Each POST has one fresh connection and one send attempt.
Explicit recipient, TLS trust, bounded headers and body, absolute deadline and
current owner admission belong to the adapter. No redirects, pooling, proxy
discovery, credential refresh or transport replay are permitted by this profile.

Wrap the raw TCP stream beneath TLS with the current-generation write fence.
Hold lifecycle admission and credential-revocation checks through the actual
socket poll. Cover all writes, including encrypted data produced by TLS reads,
flushes and shutdown. A request-level or plaintext-only check is insufficient:
Rustls may retain encrypted output after reporting plaintext acceptance.

Keep the connection driver within the owned operation. Cancellation and owner
holds retire its socket; the driver must not become an independent background
sender. Already-written bytes can still reach the remote service, so this gate
does not establish remote exactly-once execution or prove an absent effect.

Credentials are explicitly injected, scoped in-memory bearer leases. They bind
workspace, server, reference revision, profile and canonical owner authority.
Profiles serialize references only. Resolve before admitted dispatch, recheck
revocation at physical writes, and sanitize complete private response values
before capture. Optional OAuth discovery and refresh are outside this subset.

## Alternatives and evidence

Reqwest 0.12.28's connector layer wraps connection establishment but preserves its
sealed connection type and private socket. It cannot install the raw write fence
required here. Checking only a request body or wrapping the TLS plaintext stream
would leave later buffered TLS writes outside that fence.

The selected libraries are already present in the lockfile: Hyper 1.8.1,
Hyper-util 0.1.20, HTTP-body-util 0.1.3, Tokio-rustls 0.26.4 and Rustls 0.23.45.
New workspace declarations and local dependency edges expose those versions;
they do not import another MCP runtime or upgrade external packages.

The pure JSON/SSE decoder remains separate from network ownership and protocol
state. It yields one bounded frame at a time, allowing a matching reply to be
retained before later invalid input. SSE retry/ID metadata cannot authorize a
send, and an HTTP response header cannot stand in for actual request-write
observation or a matching MCP result.

## Consequences

Remote MCP configuration remains disabled until the canonical adapter supplies
durable intent, current source fences, exchange/session state and unknown-outcome
recovery. Low-level transport and credential tests are prerequisite evidence, not
the completed P7-03 acceptance gate. See the
[development contract](../development/p7-mcp-http-boundaries.md).

Reconsider this direct adapter if a higher-level HTTP library exposes an owned
raw socket boundary with equally observable writes and no hidden requests.
Broader authentication, transport or retry behavior requires separate authority
and recovery qualification.
