# Owned native terminal processes

P2-04 remains in progress. The prepared process broker supports explicitly
configured terminal profiles with bounded native qualification. See
[process authority](p2-process.md) for shared policy,
source pins, receipts and reduced-isolation limits.

`Profile::with_terminal(rows, cols)` selects a terminal with bounded dimensions.
Model requests may provide up to 32 KiB of initial UTF-8 `input`; it is included
in the immutable operation and exact grant digest. Non-terminal profiles reject
input. The strict tool schema includes a nullable input field; deserialization
still accepts its absence for existing trusted callers. Direct and PowerShell
profiles support terminal selection. Cmd terminal conversion is rejected pending
separate qualification; explicit cmd pipe execution remains available.

The launch adapter uses an ordinary executable spelling only after a native
canonical round trip confirms it names the held source path. This permits the
qualified Windows PowerShell .NET host to initialize while preserving source
identity pins; the process environment remains explicit and filtered.

The original VCP adapter uses the selected Codex ConPTY implementation, including
its retained WezTerm MIT code. An explicit owned launch assigns the child to the
VCP Job Object during creation. It disables breakaway and terminates descendants
on normal root exit. Existing upstream callers retain their prior behavior.

Only hosts with `ReleasePseudoConsole` are enabled for this path. Microsoft
documents this API for Windows 11 24H2/build 26100 and later; it permits the
terminal output pipe to close after all clients disconnect. The adapter still
releases the remaining console handle after output observation. See
[ReleasePseudoConsole](https://learn.microsoft.com/en-us/windows/console/releasepseudoconsole)
and [pseudoconsole shutdown](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session).
Other hosts receive an unsupported-isolation decision before dispatch.

Terminal output is a single merged byte stream, including virtual-terminal
control sequences and input echo. Stdout and stderr cannot be independently
reconstructed from that stream. VCP captures it in the stdout artifact and
records an empty stderr artifact. Time/output ceilings, owner close and input
guards use the same owned observer boundary as pipe processes. The terminal is
not a filesystem or network sandbox.

This increment provides bounded initial input. Follow-up input and resizing
require their own current-authority operations before they can be exposed to
the retained loop or CLI. Full loop/recovery acceptance remains with P2-05/P2-07.

Local qualification uses `scripts/test-tools.ps1`, `scripts/test-integration.ps1`,
the retained native `RecoveryTests`/`LifecycleTests`, independent source
reconstruction and fast delivery checks. The process fixture independently
records terminal handle detection and exact Unicode input, and observes actual
descendant locks, exit, deadlines and output ceilings on both canonical stores.
See the [increment evidence](../evaluations/p2-pty-increment.md).
