# Prepared native processes

P2-03/P2-04 remain in progress. The canonical host now has an original process
broker around the retained Windows Job Object launcher. It complements
[prepared file tools](p2-tools.md). Retained model-tool scheduling,
complete restart reconciliation and installed CLI integration remain subsequent
work. The P1 trusted-host process API remains a qualification interface.
The [owned terminal increment](p2-pty.md) adds bounded initial input and native
ConPTY observation; ongoing interactive control remains separate work.

## Profiles and authority

The host explicitly configures a named `vcp_tools::process::Profile`. Profiles
cannot be deserialized from model content. They name an absolute `.exe`, direct
execution or a specific PowerShell/cmd mode, public bootstrap environment values
and isolation requirements. Reopening starts with no process profiles; saved
history cannot recreate host execution authority.

Model arguments select a configured profile, argument vector, relative directory
and bounded time/output limits. They cannot supply executable identity,
environment, native capabilities or approval state. Shell profiles require one
explicit script; PowerShell disables profiles and interaction, and cmd disables
AutoRun. Direct and shell execution are both classified as opaque process, read,
write and network effects. Matching a shell prefix grants nothing.
The cmd adapter passes the explicitly authorized script using
[Windows raw command-line conversion](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.raw_arg)
with the outer quotes expected by `/s /c`; normal CRT argv escaping is unsuitable
for cmd redirection. Direct execution retains ordinary argument-vector handling.

Preparation captures executable identity/hash, directory identity, bounded
discovered source versions and exclusions, actual argv, environment digest,
profile digest and schema. Source, executable or profile changes invalidate the
ticket. Discovery respects documented exclusions; it is not an inventory of all
dynamic dependencies or external inputs an opaque program might access.
Profiles can also declare read-only script/config inputs, whose native handles
stay pinned across execution to reject substitution. Model arguments cannot
remove these dependencies. Arbitrary dynamic inputs remain part of the explicit
opaque/reduced-isolation scope, not a claim that every program dependency is immutable.
Executable root IDs must differ from the workspace root ID. Trusted read denials
are checked before reading either root, including executable identity hashing.

Workspace/plan modes do not automatically authorize opaque execution. Autonomous
policy must explicitly cover the effects and all workspace/executable roots.
Questions, waiting state and exact grants use the canonical policy service.
Answers never resume a task. A valid grant can be reused when its bound inputs
match; dispatch rechecks current ownership, policy and native facts. Unknown
workspace effects block later mutations pending reconciliation.

## Native launch and limits

The broker holds executable and directory guards, records dispatch intent, then
uses retained suspended creation, Job Object assignment and resume. There is no
uncontained fallback. A shared conflict lease excludes other broker operations
for the process lifetime. Owned handles supply authority; recorded PIDs are
diagnostic and never used to kill a recovered process.

Directory guards now request directory read access as well as attributes. An
attribute-only handle did not exclude rename in the added directory-only test.
This stricter access can reject roots whose ACL permits traversal but not listing.
The [Windows sharing contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea)
and actual native regressions define the measured boundary.

Children inherit no ambient environment. An allowlist accepts public bootstrap
settings such as SystemRoot, PATH, TEMP and CI, rejecting duplicate names and
credential variable names. The trusted host must never put credentials in these
public values. Provider authentication stays in the engine.

An independent timer terminates the job at its deadline. Both output streams
drain concurrently into canonical captures with bounded display prefixes. The
combined output ceiling triggers termination; every already-observed chunk is
recorded, with at most one 8 KiB read per stream in flight at that boundary.
Limit-triggered output is explicitly aborted/partial, with byte counts and the
stop reason. Capture failure leaves unknown effects and fences capture.

Waiting observes root exit and zero live job members before recording outcomes.
Bounded processes have an owned observer independent of the caller's wait; owner
close drains this observer before closing the lifecycle checkpoint.
The observer also owns the input guards, so dropping the caller's handle cannot
release them while the observer is still stopping and draining the process.
Root exit cannot detach background descendants. Workspace observations and
stdout/stderr artifacts remain inspectable. Failed/cancelled programs can leave
partial effects; exit does not undo or inventory every external effect. Dropped
observation records unknown state without automatic retry.

## Isolation scope

The broker enforces owned process trees, explicit environment, deadlines and
bounded output. Arbitrary filesystem/network restrictions are unavailable here.
A profile requiring them is denied even after approval. Running with the measured
controls requires an explicit `reduced_isolation` profile; no silent downgrade
occurs. General composed Windows sandbox/toolchain support retains later gates.

## Local checks

Run `scripts/test-tools.ps1` for preparation/repository regressions and
`scripts/test-integration.ps1` for canonical dispatch on both stores. Retain the
`RecoveryTests` and `LifecycleTests` commands in the
[file-tool guide](p2-tools.md#reproduction), followed by the fast suite. Final run
identities and outcomes are recorded in the [reviewed increment report](../evaluations/p2-process-increment.md).
