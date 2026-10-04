# MR-06 bounded live qualification

This runner makes four actual CLI source-edit tasks: two concurrent fixed Qwen
Google Vertex tasks, followed by two concurrent tasks using the captured model
rotation policy. Every task starts with a separate workspace and canonical data
root. All four use the same existing installed-user `LOCALAPPDATA/VCP/account`
pacing state. Each allocation is permanently $3; $12 is allocated from the
authorized $25 ceiling. Failed, interrupted, or unused allocations are never
recycled. Incomplete cost evidence or active/unresolved liability stops dispatch
of the next pair. A permanent execution claim prevents replay after interruption.

First capture **new** metadata directories with the real CLI's
`setup provider-metadata --model MODEL --endpoint ENDPOINT --output NEW_DIRECTORY`.
Use Qwen `google-vertex/us-south1`, DeepSeek V3.2 `deepinfra/fp4`, GLM 4.7
`deepinfra/fp4`, and optionally Qwen `venice/fp8`. This command does not make
inference calls. Do not edit compatibility flags or fabricate snapshots.

Run the `model_rotation_profiles` CLI crate example with output directory and
the first three metadata directories, optionally the Venice directory. It uses
the real preference/routing API to bind catalogs and choice sets: Qwen/DeepSeek
in the first choice and GLM in the second. No stronger third choice is selected.

Create a private spec outside the repository and known sync roots:

```json
{"executable":"C:/absolute/vcp.exe","node":"C:/absolute/node.exe","baseline_profile":"C:/private/profiles/baseline.json","rotation_profile":"C:/private/profiles/rotation.json"}
```

```powershell
node scripts/evals/model-rotation-live-runner.cjs prepare C:/private/spec.json C:/private/new-campaign
node scripts/evals/model-rotation-live-runner.cjs run C:/private/new-campaign/plan.json EXACT_PREPARED_SHA256
```

Preparation performs no network calls. The second command spends within the
authorized campaign budget and requires the exact prepared hash. Preserve the
existing credential boundary; never put keys in specs, profiles or arguments.
The profile grants read/write/execute/opaque for the single pinned native Node
verifier, with canonical model tools restricted to read, patch and verify.
Network/install/publish and arbitrary model process execution are excluded.
The runner freezes the executable, Node, profile, catalog, source fixture,
prompt and runner hashes. The configured check proves a CommonJS source export
changed from 41 to 42; the parent independently repeats the frozen native test
after checking immutable acceptance files and exact source content. No candidate
code is evaluated unless its complete source is exactly the expected constant.

Each CLI has a 180-second task deadline and a 240-second process deadline with
owned Windows process-tree termination. Native checks have a 10-second ceiling.
Diagnostics redact credential environment values and recognizable bearer keys;
canonical artifacts remain private. Costs, routing and output pages are saved.
Review these for served model/endpoint, 429 scope, cooldowns, waits, failovers,
receipt settlements and verification outcomes. Completion flags and parent test
outcomes are reported separately. This tiny fixture does not prove comparative
coding quality, long-context throughput, or that a shared provider pool will
produce 429s during the campaign. Fault-injected offline tests cover those paths.
