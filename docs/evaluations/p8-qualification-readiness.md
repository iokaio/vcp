# P8 qualification readiness

The P8 readiness campaign is registered in [`p8-qualification-manifest.json`](../../scripts/evals/p8-qualification-manifest.json) and recorded by [`p8-qualification-runner.cjs`](../../scripts/evals/p8-qualification-runner.cjs). It maps P8-01, P8-02 and P8-03 rows to existing native Rust tests. The runner records commit, content hashes and tracked/untracked source identity for the relevant crates, manifests, lockfile and runner/test sources, plus the exact command, working directory, exit status, stdout/stderr evidence paths and hashes. After each process exits, it reads at most 256 MiB from each evidence file and enforces a process timeout; redirected child output is not stream-capped while the process runs. It reports `pass`, `fail` and `not_run` per row; a required row with a missing prerequisite makes the campaign `incomplete`.

Run it from the repository with a new output directory:

```text
node scripts/evals/p8-qualification-runner.cjs --output artifacts/p8/<new-run>
```

Native Windows rows require a native Windows host and use `VCP_TEST_GIT` for the qualified Git executable. The campaign does not treat WSL as native Windows evidence. The packaged skills row requires `VCP_TEST_SKILL_PACKAGE` to point to the exact extracted package used by the test. The second-machine row declares inputs through `P8_AGE` and `P8_SECOND_MACHINE_FIXTURE`, but still has no complete executable acceptance command. Supplying those variables alone cannot turn it into a pass.

On 2026-09-22 the owner directed this continuation to skip machine handoff and
perform the best available validation on the current workstation. The
independent-machine row therefore remains explicitly not run at owner direction.
Local isolated roots, fresh profiles and encrypted restore remain useful
qualification, but are not reported as another Windows installation or a clean OS.
No additional environment is requested for this continuation.
Executed cases, exact package identities, corrected findings and bounded-run
limitations are recorded in the [current-machine qualification report](p8-local-qualification-2026-09-22.md).

A toolchain/build wrapper can be supplied when the host needs a pinned developer environment. The wrapper receives one final argument: the absolute path to a `p8-command/1` JSON file containing `case_id`, `cwd`, `program`, `args` and the inherited `VCP_TEST_GIT` value. The wrapper is responsible for setting up the toolchain and executing that exact command. The runner still records the wrapper invocation and captures its stdout/stderr:

```text
node scripts/evals/p8-qualification-runner.cjs `
  --wrapper C:/vcp/tools/p8-build-wrapper.exe `
  --wrapper-args-json '["--toolchain","stable"]' `
  --output artifacts/p8/<new-run>
```

This repository includes [`p8-native-command.ps1`](../../scripts/evals/p8-native-command.ps1),
which reads that exact JSON, initializes the native Visual Studio environment,
and defaults to Rust 1.98.0 through `rustup`. The local continuation explicitly
selects Rust 1.95.0, matching the P7 native checkpoint and P8 maintenance build;
record the selection instead of mixing wrapper defaults. The runner records the resulting
stdout/stderr evidence after the process exits and applies the post-run read
limit described above. Set `VCP_TEST_GIT` before
launching the runner. Cargo output defaults to `artifacts/p8-native-target`;
an explicit `CARGO_TARGET_DIR` may reuse an existing qualified build cache. Set
`VCP_TEST_SKILL_PACKAGE` to the extracted package
directory when running the packaged skills case:

```powershell
$env:VCP_TEST_GIT = (Get-Command git).Source
$env:VCP_TEST_SKILL_PACKAGE = 'C:\path\to\extracted-package'
node scripts/evals/p8-qualification-runner.cjs `
  --wrapper pwsh `
  --wrapper-args-json '["-NoProfile","-File","scripts/evals/p8-native-command.ps1","-RustToolchain","1.95.0"]' `
  --output artifacts/p8/<new-run>
```

`--case` may select rows for diagnosis. Unselected rows are retained as `not_run`, so a partial campaign cannot appear complete. `--dry-run` validates the manifest and writes explicit not-run reasons without launching Cargo. The manifest is immutable during a run; changing it or the runner causes a terminal integrity failure.

The registered native rows cover child pause/close, policy revalidation, packaged skill relocation/lazy loading/integrity, child graph durability, fresh-process history, store process-kill recovery, snapshot and restore recovery, cleanup intent retry, cleanup artifact/reference retention across both stores, real child-process kill after cleanup removal and before receipt publication across both stores, binary history inspection, compaction, vault tamper handling, writer enrollment and structured review evidence. One row remains intentionally open: exact packaged second-machine encrypted recovery with an independent recovery fixture. That required gap prevents a passing campaign until executed.

This is readiness evidence, not a release result. It does not claim package installation, owner acceptance, all fault interleavings, hardware support, cloud erasure or full U01–U09 completion.

Revision `p8-native-recovery-history-v3` additionally maps connected child and
integration write/receipt kills, individual child pause/cancel with active siblings, noisy/quiet
child output-consumer loss and cached MCP identity/authorization/prune checks.
These rows require their own current-source execution receipts. Registering them
does not extend the historical 21-row passing report or qualify packaged
compaction/reopen behavior.

Revision `p8-local-package-recovery-v4` adds opt-in exact-package full-history/purge,
independent encryption and local restore, and MCP credential-preflight rows.
The local restore is performed on this workstation and does not replace the
owner-skipped independent-machine row. The independent Go age binary must match
the digest/version in `age-qualification.json`; the native packaged crypto test
enforces that pin. Set `VCP_TEST_AGE` to that executable.
The runner assigns a fresh `VCP_TEST_P803_CRYPTO_REPORT` path inside each case
directory and retains the crypto test's metadata receipt and digest. Direct
invocation of that test must supply a new receipt path with an existing parent.
These package-only tests are ignored in ordinary Cargo runs and selected with
`--ignored --exact` by this matrix. Explicit selection still fails when its
package or required tool is absent; zero executed tests cannot pass a row.

Explicit real-model rows require `VCP_MINILM_ASSETS` to identify the existing
MiniLM asset directory. A directory prerequisite is only readiness; the native
fixtures verify the pinned bytes before inference. Rows cover CPU batch/cache,
resource accounting, pause, generation publication, sixteen process-kill
boundaries and retained-vector encrypted restore. A separate declared-long-check
row executes actual 121-second checks under trusted limits. These subsystem rows
do not imply packaged UI coverage or a minimum supported hardware envelope.
Two separate opt-in packaged rows exercise real long-check completion and pause
after native execution starts, using both stores and fresh packaged inspectors.
Retention rows cover the explicit thirty-day notice boundary, repeat notices,
owned generation cleanup and honest disclosure of retained or in-flight backup
copies after purge; none promises erasure of external copies.
Current-source native MCP rows also exercise an actual local TLS HTTP 401 and
the complete content/provenance conversation, separately from packaged startup
preflight diagnostics.
The runner binds the age/model specifications and the independent envelope
oracle along with Rust source inputs. Each row still needs its own execution
receipt; adding a row is not a passing result.
