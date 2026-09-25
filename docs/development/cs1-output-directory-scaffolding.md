# CS-1 output directory preparation

Preparation and result schema revision 3 fixes a harness feasibility defect without
changing the frozen authoring tasks, skill packages or model tool authority.
`vcp_patch` requires existing destination parents. The original package-creation
case supplied only `contract.md`; preparation added the checker files but omitted
`package/` and `package/references/`. A retained patch request for the allowed
package documents failed with Windows error 2 before any output was created.

Fresh preparation now derives the exact parent closure of original files,
immutable checker files and allowed output/edit paths. Every arm receives those
empty directories before execution. The plan binds the sorted directory list;
the runner checks it against the frozen task and actual workspace before every
arm and after execution. Missing, extra and linked directories are rejected using
the existing private-root and no-follow boundaries. Directory checks also apply
to already-started rows during subsequent validation. File identities, allowed
edits, process restrictions and canonical completion checks remain in force.

This revision cannot replay or upgrade a revision-2 campaign. Retain its original
results and preparation unchanged. Any future campaign requires a fresh directory,
current qualification and explicit authorization. Directory scaffolding is setup,
not candidate-produced evidence or a grant of a directory-creation tool.

Focused JavaScript tests cover identical empty parents across all three arms,
missing/extra/junction rejection, frozen-list tampering and final-workspace checks.
The native CLI regression uses a trusted synthetic provider and the pinned native
checker to exercise the exact package-creation paths through `vcp_patch`: absent
parents fail, prepared parents permit creation and canonical verification. It is
toolchain feasibility evidence, not a model-quality or usefulness evaluation.

The initial three-request native trial correctly failed completion: its first
`vcp_verify` selected the checker directory's instruction scope and returned
`executed:false`, without starting a check. The regression now permits one retry
only for that exact refresh response. It requires exactly four provider requests,
one executed verification and one checker process. Successful completion must
include both named TAP checks; a failed checker cannot trigger the retry. The
original failure and diagnostic rerun are retained in the local qualification logs.

Local Windows qualification passed: all sixteen focused preparation/runner tests,
all ten native checker unit tests, and the CLI regression's absent/prepared-parent
scenarios. Each scenario used four synthetic provider requests and exactly one
executed checker. The prepared case completed with both required checks; the
absent-parent case retained its patch error, failed verification and incomplete
result. Logs are retained locally under `artifacts/scaffolding-tests/`, including
`native-qualified.log` and `checker.log`. No paid model evaluation was run.

The registered `cs-authoring` case now runs the oracle, preparer and runner tests
together. The original 30-second limit timed out after registration expanded;
the unchanged combined command passed all 24 tests in 56.36 seconds locally
(the earlier preparation/runner-only run took 73.74 seconds). Its bounded timeout
is now 120 seconds. The first fast-suite attempt also retained five `not_run`
cases because the existing locked `@iarna/toml` dependency was absent. An offline
lockfile install restored that prerequisite without changing dependencies.

The focused registered case passed in run
`5d6b097c-95c4-4611-a5da-c00f455408fd`. The subsequent full fast run
`6b8e9af6-0fba-4d92-9ba3-d31d9ed6c257` passed all nineteen registered cases,
with no failed or `not_run` disposition. The earlier timeout run remains retained.
