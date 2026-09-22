# P8 qualification readiness

The P8 readiness campaign is registered in [`p8-qualification-manifest.json`](../../scripts/evals/p8-qualification-manifest.json) and recorded by [`p8-qualification-runner.cjs`](../../scripts/evals/p8-qualification-runner.cjs). It maps P8-01, P8-02 and P8-03 rows to existing native Rust tests. The runner records commit, content hashes and tracked/untracked source identity for the relevant crates, manifests, lockfile and runner/test sources, plus the exact command, working directory, exit status, stdout/stderr evidence paths and hashes. After each process exits, it reads at most 256 MiB from each evidence file and enforces a process timeout; redirected child output is not stream-capped while the process runs. It reports `pass`, `fail` and `not_run` per row; a required row with a missing prerequisite makes the campaign `incomplete`.

Run it from the repository with a new output directory:

```text
node scripts/evals/p8-qualification-runner.cjs --output artifacts/p8/<new-run>
```

Native Windows rows require a native Windows host and use `VCP_TEST_GIT` for the qualified Git executable. The campaign does not treat WSL as native Windows evidence. The packaged skills row requires `VCP_TEST_SKILL_PACKAGE` to point to the exact extracted package used by the test. The second-machine row requires explicit inputs through `P8_AGE` and `P8_SECOND_MACHINE_FIXTURE`; absent inputs remain visible `not_run` gaps.

A toolchain/build wrapper can be supplied when the host needs a pinned developer environment. The wrapper receives one final argument: the absolute path to a `p8-command/1` JSON file containing `case_id`, `cwd`, `program`, `args` and the inherited `VCP_TEST_GIT` value. The wrapper is responsible for setting up the toolchain and executing that exact command. The runner still records the wrapper invocation and captures its stdout/stderr:

```text
node scripts/evals/p8-qualification-runner.cjs `
  --wrapper C:/vcp/tools/p8-build-wrapper.exe `
  --wrapper-args-json '["--toolchain","stable"]' `
  --output artifacts/p8/<new-run>
```

This repository includes [`p8-native-command.ps1`](../../scripts/evals/p8-native-command.ps1),
which reads that exact JSON, initializes the native Visual Studio environment,
and runs it with Rust 1.98.0 through `rustup`. The runner records the resulting
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
  --wrapper-args-json '["-NoProfile","-File","scripts/evals/p8-native-command.ps1"]' `
  --output artifacts/p8/<new-run>
```

`--case` may select rows for diagnosis. Unselected rows are retained as `not_run`, so a partial campaign cannot appear complete. `--dry-run` validates the manifest and writes explicit not-run reasons without launching Cargo. The manifest is immutable during a run; changing it or the runner causes a terminal integrity failure.

The registered native rows cover child pause/close, policy revalidation, packaged skill relocation/lazy loading/integrity, child graph durability, fresh-process history, store process-kill recovery, snapshot and restore recovery, cleanup intent retry, cleanup artifact/reference retention across both stores, real child-process kill after cleanup removal and before receipt publication across both stores, binary history inspection, compaction, vault tamper handling, writer enrollment and structured review evidence. One row remains intentionally open: exact packaged second-machine encrypted recovery with an independent recovery fixture. That required gap prevents a passing campaign until executed.

This is readiness evidence, not a release result. It does not claim package installation, owner acceptance, all fault interleavings, hardware support, cloud erasure or full U01–U09 completion.
