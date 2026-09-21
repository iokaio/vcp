# U03 cart feature fixture

This is a separate frozen P7-02 generation cohort. It does not revise the existing
42-case catalog manifest. Both arms begin from identical project bytes; only the
skill arm selects `javascript-typescript`. Only `src/cart.cjs` may change.

The project has no dependencies. Its public Node tests describe two basic cases;
the independent oracle also checks discount boundaries, mixed quantities,
safe-integer overflow, invalid inputs and input immutability. It compares semantic
objects, not source text or prose. Package files, public tests and the user note
must remain byte-identical; unexpected files fail preservation.

`scripts/evals/builtin-generation-prepare.cjs prepare <spec> <new-private-dir>`
accepts the same three spec fields as the read-only runner. The supplied fixed
source profile must explicitly allow workspace read/write only. Preparation makes no
model calls, splits the proposed aggregate cap equally without recycling, binds
fixture/package/profile/executable/oracle hashes and keeps oracle expectations
outside model workspaces. Without a runtime its output is `runnable: false`.

An optional `runtime: {"node":"<absolute node.exe>","launcher":"<absolute u03-check.exe>"}`
enables the generation executor. The trusted launcher must be built from
`scripts/evals/builtin-generation-launcher.rs` with pinned Rust 1.95.0, setting
`VCP_U03_NODE` to the exact portable runtime and `VCP_U03_SYSTEMROOT` to the public
Windows bootstrap directory. The plan pins the launcher binary, reference source
and runtime identities independently. It reports
`launcher_build_provenance: owner_supplied_unverified`: hashing these files does
not prove that the binary was built from the reference source or embeds the
declared runtime path. Building and reviewing that launcher remains an owner
prerequisite before authorizing execution.
This owner-selected qualification artifact is not installed as a product tool.
The derived profile grants only read/write/execute, uses one exact launcher and
one frozen Node verification requirement. The launcher accepts only the exact
discovered public test arguments and prepends permission flags and
`--test-isolation=none`; it never grants process spawning to the test runner.

The qualified portable runtime is Node 26.9.0 Windows x64, from
[the signed upstream release](https://nodejs.org/en/blog/release/v26.9.0).
Archive SHA-256: `c8af870b5b3e9789a6cbdb30270e7c212e12d76a7afa7fc7f21b6b21cc22a71b`.
Executable SHA-256: `8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1`.
Installed Node 24 lacks network denial and the oracle refuses candidate execution
there. Native qualification under 26.9.0 checks seeded failure, a positive
38-case reference, preservation rejection, forbidden external reads/writes,
network, spawning and workers. Set `VCP_U03_LAUNCHER` to the built launcher and run
`node.exe --test src/tests/contracts/builtin-generation.test.cjs` with that runtime
to additionally check the public executable tests, rejected extra arguments and
one-shot runnable plan interruption behavior.

After an explicit live cap is authorized, run
`scripts/evals/builtin-generation-runner.cjs run <plan.json> <authorized-plan-sha256>`.
Execution checks every frozen input, records a non-replayable claim, inspects
canonical costs and dispatched skill contexts, and requires successful CLI
completion plus a successful retained parent verification before grading generated
bytes. Failed/interrupted CLI completion cannot become a pass through the oracle.
Unknown liability stops the remaining arm. Public check outcomes and oracle results
are retained separately. No live usefulness result or full CLI generation trial is
claimed by the synthetic controls; those acceptance checks remain open. No cap has
been spent.

On the qualified runtime, the oracle uses a minimal environment without
provider credentials, default-denied network/write/spawn/worker/addon permissions,
candidate-only filesystem reads, bounded memory, output and wall time. These
controls are tested on that exact runtime; they are not a hostile-code sandbox
claim. Successful oracle checks must never convert a failed CLI task into a pass.
