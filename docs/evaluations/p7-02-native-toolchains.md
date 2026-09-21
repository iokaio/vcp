# P7-02 bounded native toolchain qualification

This increment records actual local command receipts for the frozen built-in
fixtures. It is partial toolchain evidence, with zero model calls; it does not
complete live skill usefulness or the full P7-02 gate.

Run `node scripts/evals/builtin-toolchain-qualification.cjs [output-root]`.
The default output root is `artifacts/p7-builtin-toolchain`. Every invocation
creates a unique directory, verifies the manifest's exact fixture bytes, copies
only its declared files into separate case directories, and records all 42 cases.
Existing generated files (including the original .NET `obj` directories) are
preserved and excluded from execution copies. Before/after snapshots cover the
entire original fixture tree, including undeclared files, and the runner hash.

The report includes host identity, source commit, fixture revision, source file
hashes, runner hash, tool-version probes, exact arguments, working directories,
exit codes, stdout/stderr and failed or timed-out attempts. This is working-tree
evidence, not a claim that the source commit alone contains the runner. It invokes
only bounded commands from its own fixed recipes, with no install or download
step. Rust uses an already installed exact 1.95.0 toolchain and locked offline
Cargo. Command timeouts and output limits are recorded as failures. The runner
exits nonzero for any failed command or integrity error; a seeded failing check
remains a failure in the result denominator.

## Windows observation, 2026-09-21

Final observed artifact: `artifacts/p7-builtin-toolchain/run-imFxpN/manifest.json`.
Manifest SHA-256:
`079237794c6999bf29e37ede3cfbf9cfe8c997c29258832104ff4d7ee91be881`.
Runner SHA-256:
`1174d13f18a7a138c44411877755833c539b7efac3df220f11bf74b1e816c5c7`.
Host: Windows `10.0.26200`, x64. Fixture revision:
`p7-02-builtin-fixtures-v1`. Original fixture and runner bytes remained unchanged.

| Fixture boundary | Observed result |
|---|---|
| Testing unit and integration | Both passed under Node 24.21.0 |
| JavaScript workspace test | Passed; direct invocation of the declared Node script, not a TypeScript check |
| Rust checked-feature integration test | Passed under installed `1.95.0-x86_64-pc-windows-msvc`, locked/offline |
| .NET/PowerShell literal-path script | Passed under PowerShell 7.6.6; .NET build/test not run |
| Shell literal-path script | Passed under PowerShell 7.6.6 |
| C++ configure, build and CTest | Passed with MSVC 19.44.35217.0, CMake 3.31.6-msvc6 and Ninja 1.12.1; one native test executed |
| Review/debug seeded regression | Failed as observed: shipping fee returned 5 instead of 0; fixture left unchanged |
| Remaining fixtures | 35 explicitly not-run, including all 21 analysis-only negative/scenario cases |

Totals: **6 passed, 1 failed, 35 not-run, 0 runner errors**. The process exited 1
and reports `completed_with_failures`, `qualification: partial`.

Python 3.13.7 is available, but the Python fixture requires an existing 3.12
environment with pytest. .NET SDKs are installed, but the test adapter dependencies
are deliberately absent. Java 21 is available, while the Java 17 fixture's Maven
wrapper is an unavailable stub. Go, Ruby, PHP, Swift and Dart probes returned
`ENOENT`; no installation was attempted. SQL has no authorized disposable database.
The C++ run used the installed Visual Studio developer environment and its bundled
CMake/Ninja paths. No compiler was installed. Other analysis/control fixtures have no execution recipe in this runner. A detected
tool on a future host does not automatically qualify an unimplemented recipe.

Two earlier development attempts remain in ignored artifacts: `run-9CJjkQ`
recorded the same case totals before commit-identity error checking was added;
`run-hEYhgr` stopped with a source commit identity error caused by Git ownership
validation. The final runner scopes `safe.directory` to the one read-only Git
identity command without changing Git configuration. Neither earlier attempt is
the final source-qualified result. The prior `run-OqxOmX` recorded five passes,
one seeded failure and 36 not-run cases before the bounded C++ recipe and installed
Visual Studio tools were supplied. The latest run supersedes that partial evidence.

`node --test src/tests/contracts/builtin-toolchain.test.cjs` passed 4 tests covering
execution in isolated copies, source preservation, exclusion of generated files,
hash/path rejection, actual failure/missing-command/timeout receipts and refusal
to execute synthetic negative cases or fetch missing pinned Rust. During runner
testing, inherited `NODE_TEST_CONTEXT` suppressed nested Node tests; the helper
now removes that parent-worker flag before executing child checks, and the test
proves a broken copy produces a real failure.

Native coverage remains limited to the checks above. No broad ecosystem support,
model command-selection quality, .NET test success, or fresh-machine packaging
qualification is implied. Live usefulness, remaining toolchain recipes/provisioned
environments and P8 acceptance remain open.
