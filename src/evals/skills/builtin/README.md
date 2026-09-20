# Builtin skill contract fixtures

`manifest.json` freezes 42 cases across the 21 original packages in
`src/skills/builtin`: one normal project and one negative or missing-prerequisite
project per family. The initial revision is `p7-02-builtin-fixtures-v1`.
Fixtures and expectations are original VCP content under Apache-2.0.

Each `projects/<case-id>` directory contains actual, small project files. The
manifest binds every file's path, byte length and SHA-256. No fixture requires
dependency installation, a network connection or credentials. Example remote
addresses use `invalid.example`, `example.invalid` or `sample.invalid` and are
not execution targets. Wrapper stubs explicitly refuse execution; unavailable
toolchains and partial dependency inventories are deliberate fixture conditions.

## What deterministic checks establish

Run production native discovery against the builtin catalog and production
project-cue observation against each fixture root. Compare observed cues,
automatic suggestion and explicit activation to the independently predeclared
expectations. Discovery must read descriptors without reading skill bodies or
resources. Explicit activation must return the exact hashed selected body through
the normal skill boundary. All fixture files must retain their declared bytes.
Report every attempted case, including errors, in the denominator.

The cases provide `vcp_read` and `vcp_list` only. They do not claim installed
compilers, authorized process/network effects, or observed task solutions.
`toolchain-state.json` and `runner-state.json` are synthetic scenario evidence;
they do not discover or change the real host. Environment guidance in a skill
does not override actual host capability or broker policy.

Automatic-suggestion labels are authored independently of production matching
output and descriptor contents. Current root markers do not cover requirements-
only Python, solution/PowerShell-only .NET, or Kotlin-DSL-only JVM roots. The four
`explicit:*` cue families (shell, SQL, data and infrastructure) require explicit
activation with the current host. Generic skills deliberately have empty cues.
Marker presence is a suggestion, not proof of manifest validity or permission
to execute its scripts.

## What these cases do not establish

`behavior_rubric` describes the expected analysis/review/generation behavior for
a later observed task. It is not a claim that a model chose a command, found a
defect, preserved a Git index, or generated a correct change. Static content
checks and scripted responses cannot qualify usefulness. Actual language checks
need provisioned tools and observed command receipts; live usefulness needs
separately authorized evaluation and independently graded outputs.

The Git cases contain working-tree sentinel bytes and `git-setup.json` with the
required staged/unstaged/untracked or worktree-conflict setup. Testing actual Git
state requires materializing that state with the real Git adapter in a separate
temporary repository. Reading these fixtures alone does not establish index or
worktree preservation. Likewise optimization/memory JSON is explicitly labeled
scenario evidence, not a canonical VCP report or store.

Keep measured results, logs, host/toolchain identities and failed attempts in
ignored artifacts, separate from shipped coverage declarations. Changing frozen
expectations after a run requires a new fixture revision and an explained rerun.
`author-fixtures.cjs` records the original finite fixture inputs for maintenance;
ordinary qualification reads the frozen files and must not regenerate them.
