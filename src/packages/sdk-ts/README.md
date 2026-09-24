# VCP TypeScript SDK

Private workspace package for P9-03. Runtime qualification targets Node 24 and
the VCP 1.0 local API on Windows. Generated wire types and schema come from
`@vcp/protocol`; this package owns connection and consumer lifecycle.

```ts
import { launchLocal } from '@vcp/sdk';

const client = await launchLocal({
  executable: 'D:\\trusted-install\\vcp.exe',
  workspace: 'D:\\work\\project',
  data: 'D:\\private\\vcp-data',
  role: 'observer',
});
try {
  const session = await client.call('session/read', { scope: client.scope });
  console.log(session.value);
} finally {
  await client.dispose();
}
```

Use an explicitly trusted absolute executable path and an existing configured
workspace descriptor. The SDK launches `local-bridge` with private pipes; the
native bridge authenticates the server and owns its Windows process handles.
Bootstrap credentials never belong in command-line arguments. A pipe connection
can expose opaque `attachment()` and `observerAttachment()` handles for explicit
reattachment with `attachLocal`. Keep them in memory; they are not serializable
discovery records or a substitute for native peer authentication.

Calls retain their generated `{kind, value}` result envelope. Snapshot and event
calls can return a gap; callers must handle it explicitly. Exact counters and
money remain decimal strings. Optional method profiles are negotiated before use.
The `editor/prepared-edits/1` profile negotiates all five editor methods:
`editor/context`, `editor/prepare`, `editor/changeRead`, `editor/dispatch` and
`editor/changeResult`. Context and mutations require current controller authority;
preparation and dispatch additionally require a real running execution task.
Observations and replacement text remain connection-local unless explicitly
captured. Durable change records contain metadata and per-file outcomes.
Persist the prepare command ID (also the change ID) before sending. A recovered
dispatch returns `apply: false`, never permission to repeat application. Context
`closed` IDs attest actual closure of exact current observations; neither a
clean dirty flag nor an absent document list retires them. See the
[versioned editor contract and native qualification](../../../docs/development/editor-edits.md).

`task/presentation` provides bounded, access-checked task/child details, retained
model selection, approval summaries and evidence. Handle unavailable fields and
fresh-page requirements explicitly; see the [editor task contract](../../../docs/development/editor-tasks.md).

`history/query` and `memory/history` require their respective `/1` profiles and
return `history` and `memory_history` envelopes. Follow `next_cursor` unchanged;
each page checks current access and retention. History is limited to the attached
session, including taskless events when no task is selected. Memory versions use
a stable sequence window with current evidence and retention states. Summaries
and links may be explicitly truncated; full evidence requires authorized artifact
range reads. See the [inspector query contract](../../../docs/development/editor-inspectors.md).

`policy/read` negotiates `policy/inspection/1` and returns a `policy` envelope;
`routing/status` negotiates `routing/status/1` and returns `routing_status`.

Optimizer clients negotiate `routing/optimizer/1` with each requested method.
`routing/reportCapture`, `routing/apply` and `routing/rollback` return standard
durable acceptances; reconcile their original IDs after a lost reply.
`routing/reportRead` returns `routing_report`, using the capture command ID as
report identity. `routing/preview` returns `routing_preview`, including exact
proposed and effective values. Preview handles expire and do not survive reload.
Mutations pin workspace/binding revisions; previews separately pin policy
revision. A policy acceptance revision must not become the next workspace
precondition. These methods do not dispatch provider work.
Both read the current task context and invalidate continuations when relevant
policy or host facts change. Grant observations never authorize dispatch. Stored
routing policy is separate from host-effective policy, which can be unavailable.
Registry source references are metadata observations; use `artifact/read` for
fresh authorization and retained-byte verification.

Mutations require an explicitly acquired controller and the caller's durable
command identity. `newCommandId()` creates an identity only when called. After an
interruption, use `reconcile({scope, command_id})` and retain the original payload
for any deliberate retry. A successful receipt read does not acquire a controller,
answer pending input or resume execution.

An `AbortSignal` cancels a local await or event consumer. Task cancellation is a
separate revision-bound mutation. An abandoned in-flight call retains its wire
slot until its reply or transport deadline; the SDK never retries it implicitly.
Closing a controller connection invokes the native owner-loss pause contract.
`dispose()` is idempotent and cleans up the SDK's own bridge, subscriptions and
pending consumers. It does not kill a reattached server by PID.

The [examples](examples/) cover starting and inspecting work, pending input,
original-key retry, connected pause/explicit resume, child progress, gaps and
exact artifact ranges. They accept explicit identities and current revision
guards, and are exercised by the compiled-server qualification driver.

Build and check from a clean checkout:

```powershell
npm ci --prefix src/packages/sdk-ts --ignore-scripts --no-audit --no-fund
npm test --prefix src/packages/sdk-ts
```

This builds JavaScript, declarations and examples, runs bounded-runtime tests,
and type-checks a consumer through the package exports. Native Rust integration
test `local_sdk` additionally seeds both stores and runs these clients/examples
against the compiled `vcp.exe`, using synthetic local providers. It requires the
repository's native Windows qualification environment and built SDK/examples.
Neither an SDK unit test nor a TypeScript check establishes native attachment
security or provider qualification. Detailed results belong in the
[public protocol guide](../../../docs/development/public-protocol.md).

P4-01 adds `observerReconnectReference()` for pipe connections. Persist this
non-secret reference outside project-controlled settings and use
`reconnectObserverLocal({executable, reference, initialize})` after reload. Native
peer authentication issues fresh observer authority; this never recovers a
controller token or starts another writer. A stale reference fails explicitly.

`rebindLocal({executable, workspace, workspaceId, data})` performs one explicit
CLI reconciliation of retained history before attachment. It requires the active
owner to close, preserves exact string revisions, and returns no private descriptor
path. Failure or timeout never retries; inspect current state before retrying.
`workspace/setTrust` remains a revision-checked controller mutation through `call`.
