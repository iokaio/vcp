# P10-04 execution-host capability baseline

Status: source audit, September 24, 2026. P10-04 remains incomplete. The owner
excluded native Linux and macOS from current work on this date. This document
records the construction baseline required by
[P10-04](../plan/19-deferred-extensions-and-platforms.md#p10-04--other-execution-environments);
it does not qualify or advertise another execution environment.

## Implementation boundaries

The plan's proposed `vcp-exec/platform` and `vcp-repository/host_path` modules are
not the current implementation locations. Execution lives in
`src/crates/vcp-lifecycle/src/process` and the canonical `foundation` broker;
repository host operations live in `src/crates/vcp-repository/src/path.rs`.
Extend these boundaries before considering a crate split.

The CLI rejects non-Windows execution in `vcp-cli/src/main.rs`. The process
adapter, canonical process broker, coding tools and several recovery/memory
adapters are compiled only for Windows. Compiling host-independent libraries or
running the JavaScript harness on Ubuntu does not qualify the CLI there.

## Separate capability matrix

`Unsupported` means the current VCP execution path does not provide the control.
`Unknown` means qualification evidence is absent. Windows entries describe the
existing implementation and reference its tests; they are not a fresh qualification
run or an expansion of the retained P8 support claims. Every new host remains
unqualified, including controls that its operating system could theoretically offer.

| Control | Existing native Windows boundary | Native Linux | Native macOS | WSL | SSH placement | Devcontainer |
|---|---|---|---|---|---|---|
| Executable/argument launch and shell | Explicit absolute `.exe`; Direct, PowerShell, Cmd [A] | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Paths, case, Unicode, links and root identity | Local drive paths, native identity, reparse rejection, held ancestors [B] | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Task ownership and descendant cancellation | Suspended launch into bounded Job Object; termination observation [C] | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Process count, time and output bounds | Explicit broker capability set [D] | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Filesystem and network confinement | Unsupported in this broker; required controls reject dispatch [D] | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| PTY | Owned ConPTY with Job Object, runtime availability probe [E] | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Credentials and environment | Explicit filtered process environment; destination credential references [A] | Unknown | Unknown | Unknown | Unknown | Unknown |
| Local inference and native indexes | Existing Windows qualification; retained release gaps [F] | Unknown | Unknown | Unknown | Unknown | Unknown |
| Installation, upgrade and handoff | Existing Windows package/recovery evidence; retained release gaps [F] | Unknown | Unknown | Unknown | Unknown | Unknown |
| Remote operation identity and disconnect reconciliation | No remote execution-host adapter | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |

Evidence references (paths under `src/crates` unless stated otherwise):

- **A:** `vcp-tools/src/process.rs`, `Profile::new`; broker regression in
  `vcp-lifecycle/tests/support/process_broker.rs`; filtered environment tests in
  `vcp-lifecycle/tests/duplex_process.rs`.
- **B:** `vcp-repository/src/path.rs`, `native`, `HeldPath` and `Root::pin_version`;
  `vcp-repository/tests/mutations.rs`. The non-Windows native adapter explicitly
  returns `Unsupported`.
- **C:** `vcp-lifecycle/src/process.rs` and `process/launch.rs`;
  owner-loss, pause and descendant-limit cases in
  `vcp-lifecycle/tests/duplex_process.rs`.
- **D:** `vcp-lifecycle/src/foundation/worker/execution.rs`, `process_preflight`;
  `vcp-tools/src/process.rs` required isolation;
  `vcp-policy/src/lib.rs` rejects required isolation absent from host facts.
- **E:** `vcp-lifecycle/src/process/pty.rs`; availability checked by the broker
  through `codex_utils_pty::owned_pty_supported`.
- **F:** [P8 qualification follow-up](../evaluations/p8-qualification-followup-2026-09-22.md)
  and [owner-directed closure](../adr/042-owner-directed-p8-closure.md).
  Closure does not qualify another host or erase not-run cases.

## Available-host observations

Read-only discovery on September 24, 2026 found `Ubuntu-24.04`, `Ubuntu` and
`docker-desktop` WSL distributions and a `desktop-linux` Docker context. Running
`uname -sr` and reading `/etc/os-release` inside `Ubuntu-24.04` reported
`Linux 5.15.167.4-microsoft-standard-WSL2` and Ubuntu 24.04.2 LTS. Python 3 was
available; `command -v cargo`, `node` and `pwsh` returned no executable in that
shell's path. This is WSL discovery, not native Linux qualification. Listing a
Docker context establishes neither a running daemon nor a qualified devcontainer.

No new VCP process, filesystem, credential, cancellation, recovery, installation
or handoff acceptance tests were run on those placements. Native Linux and macOS
are outside the current owner-authorized scope. No SSH execution destination was
provided. Native Windows remains the existing execution target and separate
regression gate.

## Required design and qualification before enabling another host

The first path-adapter decision must preserve the existing trust boundary.
Windows `HeldPath` retains ancestor handles and denies write/delete sharing;
mutation tests require competing writes and renames to fail. Ordinary POSIX
`open`, `fstat` and `O_NOFOLLOW` do not provide that guarantee. Merely adding a
non-Windows `native` module would leave pathname-based launches and script inputs
exposed to replacement. Record a host-specific design with descriptor-relative
resolution and an enforceable input/mutation strategy before enabling dispatch.
Keep the existing Windows assertions meaningful.

Similarly, a POSIX process group is not evidence of the Job Object descendant
ownership and process-count guarantees. Qualify the actual containment mechanism,
including owner death, detached descendants, live pause and explicit resume.
Unavailable required enforcement must continue to reject dispatch; neither a
worktree nor a container label supplies authority or confinement by itself.

For each subsequently prioritized placement:

1. Name the tested OS/version, runtime, filesystem/mount and workload. Keep UI,
   canonical-store and execution-host identities explicit. WSL drive mounts,
   native Linux filesystems and recreated containers need separate evidence.
2. Resolve `(host, workspace, root, relative path)` on the execution host; qualify
   case collisions, Unicode, mount boundaries, links and explicit root rebinding.
3. Provision destination authority and credential references there. Do not copy
   ambient UI credentials or reinterpret a Windows absolute path remotely.
4. Integrate launch, cancellation and owner loss through the existing durable
   broker. Preserve unknown effects after disconnect until independently
   reconciled using an authenticated operation identity; do not replay blindly.
5. Qualify encrypted snapshot transfer, history/liabilities and destination
   rebinding. Validate schema/model/tokenizer/index compatibility; rebuild only
   derived indexes from retained canonical inputs with provenance.
6. Package actual dependencies and notices, then test installation, upgrade and
   handoff outside the checkout. Record unsupported controls and every not-run
   case alongside actual process/filesystem observations.
7. Run applicable E02/E06–E10/E16/E18 and R03/R04/R06/R08 cases and preserve the
   unchanged native Windows CLI suite when shared code changes.

No supported version range can be assigned to an additional host from this audit.
The implementation ledger must remain open until the advertised environments have
their own acceptance evidence.
