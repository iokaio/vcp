# Prepared native file tools

P2-04 is in progress. `vcp-tools` prepares read/list/literal-search and multi-file
patch operations; the canonical host owns questions, dispatch and receipts.
The [process broker](p2-process.md) extends this boundary; PTY integration,
retained model-tool scheduling and full close/
recovery acceptance remain subsequent work. The existing P1 trusted-host process
entry point is not a model-facing execution capability.

## Preparation and authority

The trusted host resolves the current canonical workspace root, actor, owner,
policy, steering and authority epochs. Model arguments cannot supply those facts.
Preparation requires workspace trust. A trusted read denial on the registered
root also blocks preparation reads; path-specific read denials conservatively
block that root's preparation until a narrower permitted root is available.

One literal function schema supplies both the gateway definition and the hash in
the prepared operation. Arguments, native source identity, complete before/after
bytes, absent destinations, limits and the proposed result are captured before
dispatch. Root access does not include Git metadata or Windows device names.
Preparation performs permitted observations but never writes candidate files.

The parser and line-ending machinery remain in the selected Codex apply-patch
crate. A pure preparation entry point requires exact, unambiguous matches while
retaining CRLF/mixed-line-ending behavior. VCP preserves UTF-8 BOM and BOM-marked
UTF-16 encoding. Unmarked non-UTF-8/binary inputs reject. Existing parent
directories are required for additions and rename destinations. Overlapping
change paths reject before any write.

Native tickets cannot be deserialized. They belong to one canonical owner and
are consumed once. The broker serializes file effects, rechecks current policy
and source versions, records dispatch intent, then applies each file through a
fresh native guard. A question commits waiting state and the proposed operation;
an answer cannot resume it. Deliberate resume and another authority check precede
dispatch. A valid exact grant can authorize the same operation again when all
bound inputs still match.

## Windows concurrency envelope

Directory handles request directory read access, reject reparse components and deny ancestor deletion while
resolving a target. An existing mutation target is opened with explicit read,
write and delete access, denying other write/delete handles; its native identity
and full bytes must match the prepared observation. Hard-linked targets reject
to avoid changing an alias outside the root. Concurrent editors that already
hold incompatible handles cause a visible conflict.

Candidate content is synced to an exclusively created staging file. Existing
files are updated through the still-held version-checked handle and synced;
their identity remains unchanged. **This staged write is not crash-atomic.** A
crash or I/O failure can leave a partial file. Creation and rename use the owned
native handle with replacement disabled, preserving a newly appeared destination.
Delete uses the owned handle. Every file has a durable intent and an observed
outcome; errors retain partial/unknown effects and never trigger automatic
rollback or replay. Staging paths are included in observations for reconciliation.
The variable-size [Windows rename structure](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
uses an explicitly terminated UTF-16 name, and the worker checks the actual
post-mutation handle path before reporting success.

This is a scoped file primitive, not a general filesystem/network sandbox or a
claim about arbitrary hostile processes. Memory-mapped writers and hardware
power-loss durability are outside the measured sharing/flush envelope. Unsupported
isolation must still be rejected independently of user approval.

## Bounded results

Reads require UTF-8 text and a declared byte ceiling. Lists reject an exceeded
entry ceiling instead of presenting a complete-looking prefix. Literal search
uses bounded discovery and reports exclusions and incomplete results. Source
and directory observations are revalidated before results are released.
Canonical artifacts preserve raw result bytes; terminal escaping belongs to the
presentation boundary.

## Reproduction

Run from the repository root with the documented native Windows prerequisites:

```powershell
pwsh -NoProfile -File scripts/test-tools.ps1
pwsh -NoProfile -File scripts/test-integration.ps1
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The tool runner requires all 28 domain/protocol/repository/policy/tool contracts
and runs the 65 retained patch library regressions. The integration runner
requires 35 host contracts, two containment regressions and the seven-request
private CLI trace. Commands emit input hashes and stage logs under ignored
`artifacts/` directories. See the [reviewed evidence](../evaluations/p2-tools-increment.md)
for measured boundaries and remaining acceptance.
