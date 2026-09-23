# ADR-056: Initial editor observer connection

Date: 2026-09-23
Status: accepted for the initial P4-01 increment; full P4-01 acceptance remains open.

## Decision

Begin the prepared workspace/connection UI with an explicit, read-only connection
through `@vcp/sdk`. The Rust engine continues to own canonical identity, policy,
leases, scheduling and pending decisions. The extension does not create a model
gateway, read private engine descriptors or assign identities from folder names.

The first increment selects an existing initialized local Windows workspace and
uses a trusted executable configured at user scope. Workspace settings cannot
select the executable or data directory. Remote and virtual roots fail before
launch. The native authenticated stdio bootstrap and SDK compatibility validation
remain the connection boundary. No task execution, approval response, controller
acquisition or automatic reconnect is exposed by this increment.

Display actual engine build, negotiated protocol, execution host, workspace,
engine trust and observed task state. Folder selection uses URI identity; equal
display names do not share authority. Root changes invalidate outstanding reads
and close the owned connection. Reconnection is explicit and reads current state;
it does not replay commands or resume tasks. Connection handles and credentials
are never serialized into workspace state or sent to the webview.

The view follows the prepared workspace/connection mock's presentation, using
bounded data, escaped text, a restrictive content security policy and a closed
set of extension-host messages. The staged extension contains the compiled SDK
and canonical schema; it must resolve without repository-relative runtime links.
This is development staging, not P4-05 installation/update qualification.

## Full-item boundary

Three API prerequisites remain before the complete P4-01 contract can pass:

1. A selected binding projection exposing the actual opaque root identity and
   binding revision. Current `WorkspaceView` exposes the root path and workspace
   revisions; the extension must not invent missing authority fields.
2. Engine-backed trust changes and explicit moved-root reconciliation. Existing
   internal commands are not yet public attachment methods, and moved-root recovery
   can be needed before ordinary attachment succeeds.
3. An authenticated observer handoff across extension-host processes. Current SDK
   attachment handles are deliberately process-local. A reloaded observer cannot
   recover a live controller's server by persisting its ticket or opening another
   canonical writer.

These are implementation dependencies for subsequent P4-01 increments, not
accepted substitutes for the original requirements. Restricted-mode read-only
inspection does not establish queued-tool trust revocation. Closing an observer
does not imply control over another owner's lease. The plan and traceability
ledger retain P4-01 as in progress.

## Verification

The [editor connection guide](../development/editor-connection.md) records the
qualified runtime, exact checks and remaining full-item conditions. No supported
remote platform, full task UI, editor edit application or release publication is
established by this decision.
