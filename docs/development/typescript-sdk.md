# TypeScript SDK

Owning item: P9-03, accepted on September 23, 2026. The authenticated local API prerequisite
P9-02 is accepted in [PR #151](https://github.com/iokaio/vcp/pull/151).
[ADR-055](../adr/055-typescript-local-sdk.md) records the SDK boundary.

The [package guide](../../src/packages/sdk-ts/README.md) documents setup, typed
calls, opaque local attachment, mutation identities and disposal. The generated
`@vcp/protocol` package remains the wire authority. SDK source is hand-written
under `src/packages/sdk-ts/src`; generated JavaScript and declarations in `dist`
are built locally and ignored by Git.

```powershell
npm ci --prefix src/packages/sdk-ts --ignore-scripts --no-audit --no-fund
npm test --prefix src/packages/sdk-ts
```

`npm test` builds the runtime and seven example modules, executes bounded
runtime tests, and compiles a consumer through the package exports. Required CI
runs these checks independently of native Windows qualification. The canonical
schema drift check remains part of the repository's fast delivery suite.

Native compiled qualification uses `vcp-cli` integration test `local_sdk` with
`--features qualification` under the repository's Rust 1.95/MSVC wrapper and
shared target. Build the SDK/examples first. The Rust fixture seeds both stores,
supplies bounded fixture data over stdin to the Node driver, and independently
checks retained canonical state and tool effects after it exits. No paid provider
or externally supplied credential is needed; execution uses a synthetic loopback
provider.

The supported profile is protocol/event/schema 1.0 and native bootstrap/ready/attach
version 1 on Windows. Each client negotiates methods and their required profiles.
The SDK exposes generated method parameter types and validated result envelopes;
editor and inspector features require their negotiated methods and profiles.
Unavailable capabilities are not inferred from an engine version label.
Node 24.21.0 is the native local qualification version;
required CI also builds and tests the package on Node 24.10.0.

P4-05 permits 65 seconds for a new local launch's readiness, covering the native
60-second aggregate startup bound and outer transport overhead. Attach/reconnect
readiness and initialization retain 10-second bounds. This accommodates canonical
replay without increasing ordinary RPC deadlines; see the
[packaged startup evidence](editor-packaging.md#qualification-record).

`RpcFailure.classification` exposes known application categories, retry guidance
and original operation identity while `.error` retains the bounded structured RPC
error. Unknown or malformed application details stay unclassified. Transport and
local await failures use `OperationError.commandId` when an operation identity is
available. SDK-authored error messages do not include raw peer frames or secrets.

Final verification passes 29 SDK tests from an isolated clean source copy and
offline lockfile install. Package dry-run packing contains the emitted entry point
and declarations (18 files); no package was published. The installed VS Code
1.138.0 executable also imports the SDK successfully using its embedded Node
24.18.1. This import smoke test is readiness evidence, not P4 extension acceptance.

Four compiled-server tests pass on both stores in 64.34 seconds. They run all
seven example modules and verify stdio/pipe attachment, observer/controller scope,
wrong-version rejection, original-command retry/reconciliation, snapshot/event
ordering and gaps, exact artifact bytes, a 128-call admission flood, twelve
abandoned iterators and disposal of an open iterator. A retained canonical child
qualifies child inspection; it does not claim a new delegated provider run.

Execution cases use a real synthetic provider: pending approval survives
reconnect without automatic continuation; explicit resume performs one independently
verified successful patch; connected pause retains the controller; replaying the
original resume leaves it paused. A gate on delivery of a genuine native acceptance
response proves an aborted await can reconcile without another canonical effect.
That gate models delayed client delivery, not native crash-before-send behavior.

Evidence logs are `artifacts/p9-sdk-clean-test.log` and
`artifacts/p9-sdk-compiled-final-tests.log`. The tested server build identifier is
`vcp/0.1.0`; generated schema SHA-256 is
`b3cd81f5d6403c99b283e046b1b125323e79f3c9989b030de07a6419596173ac`.
P4-01 has begun with the [initial editor connection](editor-connection.md) through
this SDK; complete editor acceptance remains separate.
