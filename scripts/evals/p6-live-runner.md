# P6 live task smoke runner

`p6-live-runner.cjs` invokes the real Windows CLI only after an explicit prepared
plan hash is supplied. `prepare` performs no network requests. The result is a
six-case smoke comparison, not calibrated model groups or shipping defaults.

Keep profiles, catalogs, workspace/state directories and live results in a new
private directory outside this repository. The runner rejects junctions/symlinks,
existing destinations, ancestor AGENTS.md files, embedded credential fields,
modified inputs and reuse of an execution claim. Credentials are supplied only
through the existing `OPENROUTER_API_KEY` process environment.

Create a private spec with three optional externally qualified profiles:

```json
{
  "executable": "C:/private/vcp/vcp.exe",
  "aggregate_cap_usd": "20.000000",
  "strategies": {
    "fixed_economical": "C:/private/vcp/economical.json",
    "fixed_stronger": "C:/private/vcp/stronger.json"
  }
}
```

The optional `routed` profile requires actual current live compatibility and role
evidence; missing/ineligible arms remain `not_run` in all denominators. Fixed
profiles may use `routing: null` to establish bootstrap evidence. They must use
`output_tokens: "512"`, `max_transport_retries: 0`, `max_requests` between 1 and
8, and a deadline of at most 600 seconds. Existing profile fields still apply,
including a dated catalog-derived provider snapshot, catalog file and explicit
workspace trust. The runner rebinds workspace, affected file paths and budget;
it does not manufacture compatibility, role evidence, group membership or
qualified reasoning efforts. It rejects executable checks, processes, external
tools, skills, evaluators and endpoint overrides. Production codec and policy
validation run again inside VCP before admission.

When `byte_ceiling_qualified` is false, VCP reserves against the endpoint's full
input capacity. This is honest but may exceed the per-run allocation and prevent
admission; a small empirical tokenizer probe does not qualify a global byte bound.

```powershell
node scripts/evals/p6-live-runner.cjs prepare C:/private/spec.json C:/private/new-trial
# Review the returned plan.json and its hash, including exact model identities.
node scripts/evals/p6-live-runner.cjs run C:/private/new-trial/plan.json <authorized-plan-sha256>
```

The cap is divided equally across all 18 planned runs, including unavailable
arms, rounded down to integral microdollars. Unused allocations are never recycled.
Under a shared $25 P6 authorization, a $20 task plan leaves $5 for separately
accounted compatibility work; this is an allocation example, not authorization
to spend either amount. The root coordinator must account for every live runner.

Each dispatch uses a fresh canonical store, exact frozen prompt, and exact case
files; labels are never copied to the workspace. The CLI receives `--budget-usd`,
`--autonomy plan`, and `--non-interactive`; there is no shell interpolation or
interactive intervention. A permanent claim precedes execution, so interruption
cannot silently replay a trial.

Per-run evidence includes captured JSONL, canonical `costs`, `routing`, `outputs`
inspection pages, hash-checked response bytes and final answer provenance.
Settled attempt totals must equal the canonical ledger, every charge must have a
final settlement receipt, and active/unknown liability halts subsequent dispatch.
Missing outcomes stay unsuccessful; missing costs stay unknown. Answer grading
keeps its original narrow evidence label, while `result.json` records execution
and accounting separately. No result automatically publishes a routing catalog.

Offline verification:

```powershell
node --test src/tests/contracts/p6-live-runner.test.cjs
```
