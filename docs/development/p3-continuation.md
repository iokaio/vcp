# P3-04 workspace continuation

Starting `vcp` without a command reads unfinished root tasks from the canonical
workspace. JSONL and noninteractive modes return a bounded candidate list without
requiring provider credentials or dispatching work. A console presents numbered
choices; an empty answer exits without resuming. Candidates carry task revisions,
child states and pause reasons, partial artifact references, pending input,
unsettled reservations and unknown effects. Truncated details are explicit and
remain available through inspectors. Ordering is latest task-tree activity first,
then task ID.

`vcp resume --last` selects the newest unfinished root. `vcp resume TASK
--expected-revision N` binds automation to a displayed revision. The owner checks
the selection under its exclusive canonical-store lock before recovery changes
state. Reopening and `/resume` share reconciliation, pending-input, retained hold
and canonical authority/budget/file/instruction revalidation. Neither selection
nor rebinding replays an uncertain effect. Independent child holds stay visible.

Workspace descriptors retain physical root and Git metadata identities.
Replaced, moved, missing or unverified legacy bindings require explicit
reconciliation. Use `vcp --workspace DESTINATION rebind WORKSPACE_ID` to bind
retained history to a verified local directory. This keeps the original store,
invalidates old authority and marks the workspace untrusted. An explicit resume
requires the current destination profile and trust; imported grants are not
reused. A retry repairs a descriptor interrupted after the canonical binding
commit without repeating that commit. Active owners and ambiguous destinations
are rejected.

Git metadata layouts that reference unregistered external paths retain the
repository adapter's existing restriction. Rebinding does not follow linked
worktree metadata, import secrets, copy stores, or restore a missing store.

## Qualification

Native Windows, stable Rust, Visual C++ x64 environment and offline dependencies,
2026-09-19. Commands run from `src/third_party/codex/codex-rs`, with the repository's
ignored `artifacts/codex-target` as `CARGO_TARGET_DIR`:

```text
cargo +stable test --locked --offline -j 4 -p vcp-cli -p vcp-audit --features vcp-cli/qualification,vcp-audit/qualification --tests
cargo +stable test --locked --offline -j 4 -p vcp-lifecycle --test canonical_host selected_reopen
cargo +stable test --locked --offline -j 4 -p vcp-lifecycle --test canonical_host fresh_process_history_preserves_actual_unknown_process_paused_child_and_late_charge
cargo +stable test --locked --offline -j 4 -p vcp-context --tests
cargo +stable clippy --locked --offline -j 4 -p vcp-cli -p vcp-lifecycle --features vcp-cli/qualification --tests --no-deps
cargo +stable build --locked --offline -j 4 -p vcp-cli --bin vcp
```

- CLI: 49 passed, including 10 executable cases and a native console fixture.
  Real ConPTY exercises numbered selection, blank cancellation, pause and
  inspection, changed input/AGENTS instructions, explicit answers and resume.
- Audit: 8 passed; P3-03 exact captured request, retention, cross-workspace access
  and projection rebuild regressions remain green.
- Selected reopen: 1 passed across Files/SQLite, proving a stale choice cannot
  change the task/watermark and a valid choice retains exclusive ownership.
- Fresh-process recovery: 1 passed, retaining unknown effects, paused children,
  actual process output and late charges. Context: 13 passed.
- Both-backend rebinding tests preserve the original store, reject active owners
  and ambiguous bindings, revoke trust, and repair publication after a simulated
  descriptor interruption without another binding commit. Native identity and
  registry-junction cases reject changed or redirected roots.
- Production CLI build, changed-file rustfmt, `git diff --check` and all eight
  `scripts/test.ps1 -Suite fast` cases passed. Clippy completed successfully with
  existing lifecycle argument-count/test warnings; no new CLI warning was emitted.
  The existing `proc-macro-error2` dependency retains its future-Rust notice.

The initial integrated run caught command-parser and execution-order regressions;
both were corrected before the passing final run. Ignored evidence logs retain
those attempts as `artifacts/p3-04-tests.log`, `p3-04-tests-2.log` and the passing
`p3-04-tests-3.log`; targeted logs use `p3-04-selected`, `p3-04-recovery`,
`p3-04-context` and `p3-04-resume`. HTTP fixtures are synthetic; these checks do not
claim a paid-provider campaign or later encrypted machine-handoff qualification.

P3-04's current local-workspace continuation acceptance is complete. P3-05/P3-06
retain their P5 service dependencies and later retention/encrypted handoff gates.
