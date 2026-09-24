# ADR-057: Editor trust commands and authenticated observer recovery

Date: 2026-09-23
Status: implemented; qualification recorded in the editor connection guide.

## Decision

Complete the remaining native boundaries identified by
[ADR-056](056-editor-observer-connection.md) without assigning the editor a second
writer or allowing an observer to acquire controller authority.

`workspace/setTrust` is a controller mutation with a durable command ID, expected
workspace revision and expected binding revision. It uses the existing owned
authority-change flow: fence admission, pause the task tree, drain outstanding
work, recheck controller authority and commit. Dropping the RPC waiter does not
abandon the operation. Trust changes invalidate the controller token; the editor
returns to observation and requires explicit acquisition for another mutation.
No trust change resumes tasks. Granting engine trust additionally requires current
editor trust. Merely granting editor trust does not grant canonical engine trust.

VS Code exposes a trust-grant event, while revocation restarts its extension host.
Controller connection loss fences and drains that editor's work; restored clients
are observers and cannot resume it. An observed trust revocation while the host is
still active issues the controller revoke command or closes a pending controller
connection. An observer cannot revoke another client's canonical trust: editor
trust restricts this editor's operations, and an unrelated controller retains its
own authority. This preserves the ownership contract rather than treating an
observing editor as a global policy administrator.

Moved-root reconciliation is an explicit SDK operation against the configured
trusted executable and data directory. It invokes the existing bounded native
`workspace rebind` command, which requires exclusive canonical ownership, pins the
destination, preserves durable workspace identity, resets trust and repairs the
descriptor after the canonical commit. The editor never opens private descriptors
or assigns an old identity by pathname. Active ownership causes refusal, not
takeover. Retrying requires inspection; the SDK never replays a failed mutation.

## Observer handoff

Extend [ADR-044](044-controlled-local-process-bootstrap.md)'s process-local attachment with a
separate observer-only bootstrap. A serializable reference contains endpoint,
server process pin and workspace/session scope, but no ticket. These values are
discovery hints. Every connection authenticates actual kernel pipe peers and
checks live process identity, executable file identity, user SID and token session.
The server forces observer authority and checks the selected scope. A same-user
process using another executable cannot use this credential-free route.

Existing ticket attachments retain their role ceilings. The new readiness field
requires an explicit bootstrap opt-in so older strict readiness decoders retain
their original shape. The dedicated observer reconnect route always returns a new
non-secret reference. It never falls back to launching another canonical writer.

The extension persists only the non-secret reference, folder URI and a digest of
its User-configured engine profile. Reload reuses current User settings, validates
the selected folder and authenticates the reference before querying fresh state.
Neither controller attachments nor provider credentials enter workspace/webview
storage. A changed profile, missing folder or dead/replaced server cannot resume
work or trigger a hidden launch. Explicit disconnect clears recovery; extension
host disposal preserves it. Temporary reconnect failure retains the reference.

The pipe server's existing 30-second idle retention applies. A live controller
keeps its server alive while observing editors reload. An expired idle reference
requires explicit connection; persistence is not an indefinite server lifetime.

## Qualification

See the [editor connection guide](../development/editor-connection.md) for native
peer-denial tests, durable trust/admission tests and actual editor-host evidence.
P4-02 task interaction, P4-03 edits, P4-04 inspectors and P4-05 installation and
distribution remain separate acceptance items.
