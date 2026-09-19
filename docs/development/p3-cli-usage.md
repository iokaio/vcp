# Structured native CLI

P3-01 supplies the Windows `vcp` executable. P3-02's interactive terminal and
P3-03's paged artifact-content inspectors are separate work items.

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

Pause/cancel reach the live owner's current-user-only Windows named pipe. Scope,
identity, epoch, revision and idempotency are checked by the canonical stop path.
An unreachable owner is an explicit error and never creates another writer.
`--control-stdin` additionally accepts newline-framed private `CommandEnvelope`
values on an inherited input handle, with the same checks and bounded framing.
Read-only commands use the live owner when the canonical store is locked.

Inspect views are `context`, `prompts`, `outputs`, `routing`, `policy`, `tools`,
`costs`, `verification`, and `memory`. P3-01 returns bounded records and artifact
references, with explicit truncation. Memory is not ready; routing reports the
fixed qualified-model capability. Full artifact browsing belongs to P3-03.

JSONL version 1 emits `accepted`, `event`, `required_input`, `cursor_gap`, and one
final `result`. Task results carry scope, all observed conditions, and a durable
receipt. Read/control results carry `data` with a watermark or command receipt;
their success does not assert that a task completed. Diagnostics use stderr.
Output loss closes the owner and durably pauses pending/running tasks; no final
line can be promised to a disconnected consumer. Ctrl+C requests cancellation;
console close uses the existing durable owner-loss pause policy.

Exit codes: 0 verified completion/command success; 1 internal or output failure;
2 invalid configuration/control delivery; 3 failed verification or incomplete;
4 required input; 5 exhausted budget; 6 cancelled; 7 unresolved effect; 8 saved
pause. Task conditions use precedence `7, 6, 5, 4, 3, 2, 1, 8, 0` and retain
simultaneous conditions. Forced process termination may provide neither a VCP
exit code nor a final line.

Qualification: `./scripts/test-p3.ps1 -Toolchain stable`. It records exact source
hashes and native tool versions, exercises both canonical backends and the
actual executable with synthetic HTTP responses, and parses its output in Node.
