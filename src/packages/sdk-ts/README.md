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
Unsupported editor methods stay unavailable until their owning implementation.

`task/presentation` provides bounded, access-checked task/child details, retained
model selection, approval summaries and evidence. Handle unavailable fields and
fresh-page requirements explicitly; see the [editor task contract](../../../docs/development/editor-tasks.md).

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
