# MCP duplex process prerequisite

P7-03 needs a process transport that preserves the P2 broker's authority and
recovery rules while exchanging bounded messages over stdin and stdout. The
existing command adapter closes stdin and captures stdout as command output;
it cannot carry an MCP session.

The prerequisite adds owned duplex IO to the existing Windows Job Object and
lifecycle boundary. Native executable identity, explicit environment, process
limits and current authority remain required. The canonical host records process
intent before launch and retains the process outcome independently of any future
MCP call result. Interrupting observation preserves uncertainty and cannot replay
the process or a remote operation.

A local server is an opaque process: it can produce effects between protocol
requests. Its workspace conflict claim therefore lasts until the owned process
tree is quiescent. Future MCP calls can use that connection's claim, but each call
still needs separate current authority, argument provenance, durable intent and
receipt. A connection is not an exemption from task completion or owner shutdown.

The canonical write entry point remains internal. A qualification-only entry
point exercises actual pipe traffic; it is not available in production builds.
Approving startup cannot authorize arbitrary later stdin content. Future MCP
integration must supply per-message admission before using the internal writer.

`scripts/evals/duplex-qualification.ps1` runs native pipe contracts, canonical host
duplex cases and the existing process broker regression. Run it from the configured
native Rust/MSVC environment. It records source identity before and after, actual
test names, toolchain identity and log hashes, and rejects zero-test selections.
The existing process lifetime cap remains 120 seconds; the adapter must close or
deliberately reconnect rather than silently renew a connection's authorization.

The [native prerequisite qualification](../evaluations/p7-03-duplex.md) passed.
This prerequisite alone does not advertise MCP
support. Protocol negotiation, server configuration, complete schema validation,
remote transport, credentials, resource access and per-call reconciliation remain
part of P7-03. No SDK is added for the process primitive.

The intended adapter will pin MCP 2025-11-25 and qualify its
[lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
and [stdio/Streamable HTTP transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).
The retained `codex-rmcp-client` is not a drop-in adapter: its sealed launcher and
session-expiry operation replay conflict with the VCP broker. The already pinned
`rmcp` library's stdio reader also requires a bounded framing boundary before
parsing. SDK adoption and protocol claims require their own observed fixtures.
