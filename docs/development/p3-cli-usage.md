# Structured native CLI

P3-01 supplies the Windows `vcp` executable, P3-02 supplies its interactive
terminal, P3-03 supplies paged evidence inspection, and P3-04 supplies workspace
continuation and explicit local rebinding.

Starting `vcp` without a command discovers unfinished tasks. A console offers a
numbered chooser; Enter leaves all tasks paused. `--format jsonl` or
`--non-interactive` returns candidates without prompting or provider access.
Each candidate includes its expected revision and recovery/accounting summary.
Use `vcp resume TASK --expected-revision N` to reject a stale selection, or
`vcp resume --last` to explicitly select the most recently active unfinished root.

If a root moved or a legacy binding is unverified, use
`vcp --workspace DESTINATION rebind WORKSPACE_ID`. Rebinding preserves history in
the existing data root and invalidates old execution authority. It does not
resume work; update the destination's user-owned profile and explicitly resume
after inspection. See [continuation qualification](p3-continuation.md).

Build from `src/third_party/codex/codex-rs` in a native Visual C++ x64 environment:

```powershell
cargo +stable build --locked --offline -p vcp-cli --bin vcp
```

Use the executable in the configured Cargo target directory. Do not enable the
`qualification` feature for production: it exists only for synthetic loopback
subprocess tests. The default executable rejects its endpoint override field.

## Explicit startup configuration

Default data is `%LOCALAPPDATA%/VCP`; override with `--data-dir`. Data and the
user profile must be outside the workspace, repositories, and known OneDrive
roots. Existing junctions are resolved before admission. Add other synchronized
directories to `sync_roots` in the profile. These are local plaintext records.
The CLI does not import project configuration as execution authority.

The default profile is `<data-dir>/profile.json`; `--config` selects another
explicit user-owned file. Version 1 uses JSON and rejects unknown fields.
Its fields are:

| Field | Meaning |
| --- | --- |
| `version` | `1` |
| `workspace` | Absolute workspace directory |
| `trust_workspace` | Explicit `true` before execution |
| `sync_roots` | Optional array of additional synchronized directories |
| `maximum_autonomy` | `plan`, `ask`, `workspace`, or `autonomous` ceiling |
| `automatic_effects` | Explicit permitted effect classes from the canonical policy |
| `budget_usd` | Positive decimal USD string, or `null` to require the run flag |
| `provider` | Complete qualified P2 `vcp_models::catalog::Snapshot` value |
| `catalog` | Absolute path to the exact raw endpoint catalog backing that snapshot |
| `affected_paths` | 1–256 relative workspace paths for instruction scope |
| `max_requests` | Cumulative root request limit, 1–128 |
| `deadline_seconds` | 1–3600; greater than 120 when acceptance runners are configured |
| `processes` | Explicit native executable profiles; no implicit shell or PATH discovery |
| `checks` | Explicit acceptance requirements, or `[]` for cited analysis |

The provider snapshot must still be current and match the captured catalog.
This CLI consumes P2-qualified provider metadata; it does not invent pricing or
qualify an arbitrary endpoint. Provider credentials come only from
`OPENROUTER_API_KEY`, are passed to transport, and are not stored in the workspace
descriptor or JSONL stream. Do not put credential values in arguments or profiles.

A process entry has `name`, absolute `executable`, explicit `environment` map,
`required_isolation` array, `reduced_isolation` boolean, and `inputs` array.
Reduced isolation requires that explicit user choice. Each check has `manifest`
(relative `package.json` or `Cargo.toml`), `runner` (`node` or `cargo`), `profile`
(process entry name), nonempty `expected_tests`, and `rationale`. Project scripts
are observed as data; only these explicit profiles can authorize execution.

## Commands

```text
vcp --workspace <directory> run "Objective" --budget-usd 2.50 --autonomy ask
vcp --workspace <directory> run --file task.txt --budget-usd 2.50
vcp --workspace <directory> resume <task-id>
vcp --workspace <directory> resume --last
vcp --workspace <directory> sessions list
vcp --workspace <directory> sessions resume <session-id>
vcp --workspace <directory> sessions fork <session-id> --through-turn <turn-id>
vcp --workspace <directory> tasks status <task-id>
vcp --workspace <directory> tasks pause <task-id>
vcp --workspace <directory> tasks cancel <task-id>
vcp --workspace <directory> inspect <id> --view verification
```

All commands accept `--format jsonl`. Task files are bounded UTF-8 input, never
shell scripts. `--non-interactive` returns required input without inventing an
answer; structured mode also does not prompt. Resume retains the canonical cap
and revalidates the current environment. A fork requires a completed canonical
turn and a persisted cap for its independent root. It quotes bounded history
through that turn under the new scope; permissions, effects and liabilities are
not imported. Oversized or unavailable historical evidence fails closed.

Text execution with terminal stdin/stdout/stderr opens the interactive workflow.
Use `/pause` (or Ctrl+C), `/resume`, `/cost`, `/history`, `/agents`, `/status`,
`/inspect <id>`, `/read <artifact-id> <byte-offset>` and `/next`. Plain text queues
durable guidance and keeps the task paused until deliberate `/resume`.
`/answer <question-id> allow|deny` records a decision without resuming. `/cancel`
ends work and `/exit` preserves a pause. `/memory` and `/optimize` report not ready.
See [terminal behavior and native qualification](p3-terminal.md).

Pause/cancel reach the live owner's current-user-only Windows named pipe. Scope,
identity, epoch, revision and idempotency are checked by the canonical stop path.
An unreachable owner is an explicit error and never creates another writer.
`--control-stdin` additionally accepts newline-framed private `CommandEnvelope`
values on an inherited input handle, with the same checks and bounded framing.
Read-only commands use the live owner when the canonical store is locked.

Inspect views are `chain`, `context`, `prompts`, `outputs`, `routing`, `policy`,
`tools`, `costs`, `verification`, and `memory`. P3-03 returns an `InspectionPage`
in result `data`: task/workspace/session `scope`, `source_watermark`, `view`,
`items`, explicit `gaps`, and `next_cursor`. Session/status commands retain their
existing `records` shape. Memory search is not ready (P5-06); routing exposes
existing attempts and captured evidence with an explicit fixed-model limitation.
Requested model identity is separate from served identity; the latter is not
normalized in canonical attempts and must be read from captured response evidence
when the provider actually reported it.

```text
vcp inspect <task-or-effect-id> --view chain --limit 32
vcp inspect <same-id> --view chain --limit 32 --cursor '<next_cursor JSON>'
vcp inspect <artifact-id> --view prompts --offset 0 --length 65536
```

`chain` includes canonical relationships and chronological events, so tool
proposal/authority, dispatch, outcome, request/reservation and verification can
be followed without executing anything. Other views filter canonical records;
context and policy evidence artifacts retain manifests, instruction/path
provenance, omission reasons and prepared policy receipts. Use each artifact ID
with `--offset`/`--length` to read that evidence. Collection-qualified references
identify records; use their task scope with the corresponding view to inspect
shared workspace authority.

Pages contain at most 128 items and 512 KiB of record/event payload. Pass the
returned cursor unchanged with the same target, view and limit. A changed
canonical watermark, authority or retention scope requires restarting without a
cursor; pages never silently mix revisions. Oversized individual records/events
are explicitly marked truncated.

Artifact reads return at most 64 KiB, with exact `bytes` (`byte_array` encoding),
optional valid UTF-8 `text`, byte offsets, descriptor and `next_offset`. Continue
until `next_offset` is null to retrieve the retained content. These are captured
bytes, never a reconstruction from current workspace files. Split UTF-8 and
binary content remain lossless in `bytes`; terminal output JSON-escapes controls.
Pruned and missing content return a gap with source identity, not a fabricated
empty result. Aborted/pending capture and excluded authentication/recovery
material have explicit omission/redaction markers. Current workspace, task
access and retention are checked for every read, also through the live owner.

Range memory is bounded. The existing spool verifies the whole artifact digest
on each range read, so I/O still scales with artifact size. No unauthenticated
seek index or second transcript database is introduced.

Qualification and scope: [P3-03 evidence inspectors](p3-inspection.md).

JSONL version 1 emits `accepted`, `event`, `required_input`, `cursor_gap`, and one
final `result`. Task results carry scope, all observed conditions, and a durable
receipt. Read/control results carry `data` with a watermark or command receipt;
their success does not assert that a task completed. Diagnostics use stderr.
Output loss closes the owner and durably pauses pending/running tasks; no final
line can be promised to a disconnected consumer. Ctrl+C requests cancellation;
console close uses the existing durable owner-loss pause policy. In interactive
text mode Ctrl+C pauses and leaves the CLI open; `/cancel` explicitly cancels.

Exit codes: 0 verified completion/command success; 1 internal or output failure;
2 invalid configuration/control delivery; 3 failed verification or incomplete;
4 required input; 5 exhausted budget; 6 cancelled; 7 unresolved effect; 8 saved
pause. Task conditions use precedence `7, 6, 5, 4, 3, 2, 1, 8, 0` and retain
simultaneous conditions. Forced process termination may provide neither a VCP
exit code nor a final line.

Qualification: `./scripts/test-p3.ps1 -Toolchain stable`. It records exact source
hashes and native tool versions, exercises both canonical backends and the
actual executable with synthetic HTTP responses, and parses its output in Node.
