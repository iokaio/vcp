# Observed offline CPU embedding qualification

This P0-02 gate runs the real `vcp-embedding` qualification executable inside a
native Windows AppContainer. It supplies ten pinned public MiniLM assets and
checks independent reference vectors, padding, truncation and model reopen.
[ADR-008](../adr/008-local-governed-memory.md) and the
[memory design](../architecture/memory-retrieval-design.md) require local inference
without a remote fallback. This experiment supplies bounded OS evidence for that
requirement; it is not the P0-05 product sandbox or a production resource envelope.
See [executed results](../evaluations/p0-02-offline-embeddings.md).

## Run and prerequisites

After [explicit asset acquisition and native setup](local-embeddings.md), run:

```powershell
pwsh -NoProfile -File scripts/test-embeddings.ps1 -AssetsRoot $modelRoot -Offline
```

The wrapper preserves its existing source verification, three library tests,
native build, dependency closure check and ordinary four-check inference run.
`-Offline` adds the observed five-phase gate. It uses Rust 1.98.0, Node 24 or
later, PowerShell 7 and the Windows APIs below. A same-host RFC1918 IPv4 interface
must be available. A missing prerequisite is `not_run`; unavailable isolation or
blocked unrestricted controls cannot yield a pass. No administrator token,
machine firewall change, provider credentials or paid service is required.

The observer can also run against an explicitly built executable:

```powershell
node scripts/upstream/trace-offline-embeddings.cjs --binary artifacts/embedding-target/x86_64-pc-windows-msvc/release/qualify.exe --assets $modelRoot --output-root artifacts/embeddings/offline
```

This direct invocation verifies assets and records binary/source hashes but does
not rebuild or qualify the dependency closure. Use the wrapper for delivery.

## Independent observations

`scripts/upstream/trace-offline-embeddings.cjs` owns a TCP listener on the chosen
local interface and an ephemeral port. Each phase receives a random 32-digit
nonce. The listener allows at most four simultaneous connections, 128 bytes per
connection and three seconds per socket. Its final transcript must contain
exactly the two distinct unrestricted control markers. No DNS or external host
is involved. The interface address is excluded from the published summary.

1. An unrestricted child connects and writes its control nonce.
2. A restricted child attempts the same connection, loads the verified model,
   executes all four checks, then attempts the connection again.
3. A fresh restricted child receives its own model copy with `config.json`
   removed. Its network attempt must be blocked and loading must return the
   structured `missing_asset` error with native exit 3.
4. Another fresh restricted child receives a same-length, one-byte corruption of
   its own `config.json`. The result must be `invalid_asset`, SHA-256 mismatch,
   and native exit 1. The original acquired assets are never modified.
5. A second unrestricted child connects with a new control nonce.

The Rust canary accepts only literal private IPv4 addresses and nonzero ports.
Connection and write deadlines are two seconds. A restricted connection that
succeeds emits its marker and fails the test. A denied connection needs either
WSAEACCES/10013 or a timeout, plus a successful Windows isolation diagnostic
identifying a missing network capability. Refusal, routing failures and diagnostic
errors do not qualify. The parent additionally requires observed AppContainer
identity, zero capabilities, both live controls and the independent transcript.
Neither a timeout nor the diagnostic alone proves isolation: an unrestricted
desktop process can connect while the diagnostic still reports a missing capability.

`src/tests/support/offline-embeddings.cjs` validates process outcomes, complete
capture, cleanup, asset identity and finite numerical differences at most 1e-5.
It preserves the fault attempts as failed native executions with
`expected_rejection: true`; it does not rewrite them as successful processes.
Its registered regression tests reject incomplete observations and false positives.

## Native process and file ownership

`src/tests/support/windows/offline-embedding.ps1` is the trusted broker. The
original `AppContainerFixture.cs` wrapper creates a unique disposable profile,
verifies its OS-returned folder against that profile's expected path and rejects
a reparse-point root. Only the binary and ten explicitly selected public model
files are copied there. Every model copy is size/hash checked before fault
injection. The owned profile lies outside the checkout.

The child starts suspended with an explicit three-handle stdio inheritance list.
Before resume, the broker puts it in a kill-on-close job limited to one active
process and reads its actual token. Restricted runs require AppContainer=true,
zero capabilities and a SID equal to the newly created profile. Unrestricted
controls must have AppContainer=false. The broker waits at most 30 seconds,
records exit status and peak job committed bytes, and releases all owned handles.
Peak committed bytes are not resident RAM, mapped bytes or a supported RAM limit.

The existing delivery harness bounds broker output to 1 MiB and its process tree
to 90 seconds. The Node bootstrap inherits the harness's environment allowlist;
only `USERPROFILE` and `LOCALAPPDATA` are restored to their original Windows
profile paths before starting PowerShell, because Windows caches those paths.
The C# constructor checks them against OS known folders. `HOME` and `APPDATA`
remain isolated. The native child receives a separate explicit environment with
no provider/proxy credentials. Its Windows local-app-data base is retained for
the OS's AppContainer path translation; temporary files use the owned profile.

Normal completion and exceptions delete only the profile created by that broker,
then verify its folder is gone. Each phase journals its exact profile/root and
cleanup state under the ignored `private/` evidence directory. Forced broker
termination can prevent managed cleanup; a missing/pending outcome fails the
gate and requires cleanup of that exact journaled profile through the Windows
profile API. Never sweep other profiles or recursively delete a guessed path.
This limitation is not a product crash-recovery implementation.

## Evidence and maintenance

The outer manifest records tools, source identity, binary/fixture/helper hashes,
five child manifests, actual token observations, numerical results, error types,
network markers and cleanup results. Configuration and ownership journals remain
under `private/`, excluded from CI artifact upload; do not publish their local
paths or interface address. No model weights are uploaded. A failed or interrupted
record is retained and cannot count as passing evidence.

This adds a network/clock effect to the **qualification binary** in the static
boundary catalog. The embedding library remains file-only. No third-party source,
dependency versions, source reconstruction patches or model selection change.
Future library updates must rerun this gate. Full-index AppContainer compatibility,
larger corpus measurements and durable recovery are separate acceptance work.

## Windows API references

- [AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)
  defines the process isolation boundary used by this experiment.
- [CreateAppContainerProfile](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-createappcontainerprofile),
  [GetAppContainerFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-getappcontainerfolderpath)
  and [DeleteAppContainerProfile](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-deleteappcontainerprofile)
  govern creation, exact owned storage and cleanup.
- [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
  supplies the security capabilities and handle list;
  [GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)
  observes the actual token before execution.
- [NetworkIsolationDiagnoseConnectFailureAndGetInfo](https://learn.microsoft.com/en-us/windows/win32/api/networkisolation/nf-networkisolation-networkisolationdiagnoseconnectfailureandgetinfo)
  and [NETISO_ERROR_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/networkisolation/ne-networkisolation-netiso_error_type)
  define the diagnostic result checked alongside live controls.
- [JOBOBJECT_EXTENDED_LIMIT_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information)
  defines the job limits and peak committed-memory field used here.
