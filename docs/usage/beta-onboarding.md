# Internal beta: first useful task and metadata renewal

BETA-03 provides production `vcp setup` commands. Use the installed Windows x64
`vcp.exe`; these commands do not require a checkout, developer fixtures, Node,
Rust, or a qualification build. The accompanying installation walkthrough and
known-issues document identify the supported release pair and its current limits.
These instructions assume PowerShell and an existing
OpenRouter account with permission and funds for the exact selected provider.

`setup provider` performs paid inference. Its `--budget-usd` explicitly admits
one fixed probe pair, at most two requests with no inference retries, capped at
25 USD per invocation. Select a cap within your separately authorized test
budget. Each later task has a separate task cap. Neither cap is a daily or
account-wide spending limit. Offline profile creation and checking spend nothing.

## Select local roots and enter the credential

Setup does not add VCP to PATH. Select the stable launcher below; for a custom
program directory, replace `$vcp` with its absolute path. For portable use, select
the `vcp.exe` in the extracted payload instead.

Create or select a workspace containing the files you want VCP to inspect.
Use a separate private, nonsynchronized directory for metadata and profiles.
The default durable history root is `%LOCALAPPDATA%\VCP`; installed packages may
select the installer's registered data root. Keep profiles and data outside
repositories and sync roots. The command rejects known OneDrive roots, network
drives, and redirected setup paths. Declare additional sync roots in your trusted
profile before task execution.

```powershell
$vcp = Join-Path $env:LOCALAPPDATA 'Programs\VCP\vcp.exe'
& $vcp --version
if ($LASTEXITCODE -ne 0) { throw 'Select the installed VCP executable before continuing.' }
$workspace = 'C:\work\beta-sample'
$private = Join-Path $env:LOCALAPPDATA 'VCP\profiles'
New-Item -ItemType Directory -Force -Path $workspace, $private | Out-Null
# Add a README.md to this sample directory using your editor before continuing.
$secret = Read-Host 'OpenRouter API key' -AsSecureString
$credential = [pscredential]::new('OpenRouter', $secret)
$env:OPENROUTER_API_KEY = $credential.GetNetworkCredential().Password
$credential = $null
$secret = $null
```

The masked prompt keeps the value out of shell command history. It remains in
this process environment for transport and inherited VCP processes. Never put
the key in command arguments, profile JSON, workspace settings, package content,
or diagnostic output. Remove it after the final task using the cleanup command
at the end of this walkthrough.

## Qualify the exact endpoint

Choose an exact model ID and endpoint **tag**, not its display name. Inspect the
current model endpoint catalog in the provider UI or its
[documented endpoint API](https://openrouter.ai/docs/api/api-reference/endpoints/list-all-endpoints-for-a-model).
The selected endpoint must support tools, tool choice and output limits, have
usable context/output limits and explicit pricing. Setup rejects ambiguous
provider pools. The fixed request denies provider data collection, disables
fallbacks and sets explicit price ceilings. It does not request ZDR; use this
beta only where that policy is acceptable. Setup contains no custom endpoint,
proxy, or qualification bypass setting.

Fill these variables with the approved exact selection and caps. No credential
belongs in them. `Read-Host` here is for nonsecret choices only:

```powershell
$model = Read-Host 'Exact model ID (organization/model)'
$endpoint = Read-Host 'Exact endpoint tag'
$requestPrice = Read-Host 'Maximum USD per request fee (for example 0.001)'
$probeBudget = Read-Host 'Authorized total probe budget in USD'
$generation = Join-Path $private ('provider-' + [guid]::NewGuid().ToString('N'))
& $vcp --workspace $workspace setup provider --model $model --endpoint $endpoint `
  --request-price-limit $requestPrice --budget-usd $probeBudget --output $generation
if ($LASTEXITCODE -ne 0) { throw 'Provider setup did not qualify; inspect the recovery table below.' }
```

Success reports `status: qualified`, the catalog and snapshot paths, actual
settled cost, and expiry. Setup captures the catalog, uses the canonical budget
ledger for each call, verifies the fixed tool call and continuation, and joins
the corresponding [generation receipts](https://openrouter.ai/docs/api/api-reference/generations/get-request-&-usage-metadata-for-a-generation)
to the exact endpoint, model revision, and cost. It never treats the requested
provider pin as proof of the provider that actually served the request.

The snapshot lasts 12 hours from catalog capture. This is bounded metadata
validity, not a model quality or tokenizer qualification. Conservative admission
reserves the endpoint's full input capacity, so a small cap can reject a call
even when the fixed prompt is short. Increase a cap only within the approved
budget; setup never retries with a larger cap or cheaper provider automatically.

## Create a profile and run a read task

```powershell
$profile = Join-Path $private ('beta-sample-' + [guid]::NewGuid().ToString('N') + '.json')
$taskBudget = Read-Host 'Authorized per-task budget in USD'
& $vcp --workspace $workspace setup profile `
  --snapshot (Join-Path $generation 'qualified\snapshot.json') `
  --catalog (Join-Path $generation 'endpoints.json') --output $profile `
  --trust-workspace --budget-usd $taskBudget --autonomy ask --affected-path README.md
if ($LASTEXITCODE -ne 0) { throw 'Profile creation failed.' }
& $vcp --workspace $workspace --config $profile setup check
if ($LASTEXITCODE -ne 0) { throw 'Profile preflight failed.' }
& $vcp --workspace $workspace --config $profile run `
  'Read README.md and summarize its purpose with citations. Do not change files.' --autonomy ask
```

`--trust-workspace` is required and binds this profile to that exact workspace.
Only reads are automatic in the generated policy; the default tool ceiling
includes repository reading, search, patch and source-integrity verification.
External process execution and MCP are absent. The profile limits each root task
to eight model requests, 2,048 output tokens per request, zero transport retries,
and a five-minute task deadline. A denied effect needs explicit applicable user
approval; changing the requested autonomy cannot exceed the profile ceiling.

The first accepted task registers the workspace and durable history. Starting
`& $vcp --workspace $workspace` without `run` only discovers unfinished tasks.
Reopening does not resume a task. Use the displayed task ID and revision for an
explicit resume; the original task's budget and spent amount remain in force.

Each workspace gets its own profile filename. Always pass its matching
`--workspace` and `--config`; the legacy global `profile.json` default cannot
represent several workspace bindings. For VS Code, select the installed CLI and
data directory in **User** settings, select this execution profile through
**VCP: Start Execution-backed Task**, use its credential input, and review
workspace trust there. Never copy the key to settings. See the packaged VSIX
walkthrough for its exact supported settings and connection flow.

## Add real checks and tools deliberately

The generated `checks: []` means source-integrity verification and cited
analysis, not execution of a test suite. For code edits, review the profile in a
local editor and add the required absolute executable `processes` and named
`checks`, following [the profile field contract](https://github.com/iokaio/vcp/blob/main/docs/development/p3-cli-usage.md).
Choose each process's filtered environment, inputs, isolation and timeout
explicitly; no shell or PATH discovery supplies missing tools. Add `vcp_exec` to
the canonical tool ceiling only when execution is intended. `setup check`
validates configured executables and check references, but does not run tests or
claim that the toolchain is installed correctly. Run the actual project checks
and inspect their evidence during the task.

For imported preferences, use the existing configuration import preview/apply
workflow after profile creation. A new base profile generation does not silently
inherit an older base's import revisions. Re-preview and explicitly apply the
preferences you still want. Keep the prior profile and receipts for inspection.

## Renewal and failures

Repeat `setup provider` into a **new** directory when metadata expires, then
create a **new** profile filename and reapply reviewed process/check settings and
imports. Select the new profile explicitly in the CLI and VS Code. Never edit
timestamps, prices, compatibility flags or hashes to extend old evidence.
Workspace history stays in its existing data root.

| Observed condition | Required next step |
| --- | --- |
| Missing credential | Enter it with the masked prompt or the extension credential input in the process that launches VCP. No call is made without it. |
| Credential rejected or metadata HTTP failure | Check the account and key privately. No automatic retry occurs. A provider call that already started may retain liability. |
| `result.json` is observed but generation receipt is temporarily unavailable | Run `& $vcp --workspace $workspace setup provider-complete --directory $generation` while metadata is current. It fetches only missing receipts, verifies the original evidence, and never repeats inference. |
| Provider probe failed, interrupted, or has unresolved liability | Preserve the directory and canonical ledger. Reconcile the original request with provider billing/support before authorizing a fresh probe budget. Never regard missing observed cost as zero. `provider-complete` cannot replay failed calls. |
| Stale snapshot, changed catalog, unsupported pricing or ambiguous served identity | Obtain a newly qualified generation; review endpoint eligibility and caps. No snapshot is published from incomplete or mismatched evidence. |
| Profile belongs to another workspace | Select that workspace's profile or create a separately trusted profile. Do not rewrite another workspace's profile. |
| Output already exists | Choose a new generation name. Setup never overwrites profile, metadata or retained history. |
| Missing executable or check profile | Install the required tool explicitly or correct its absolute path and configured check. Run `setup check` again. |
| Denied write, execution or network effect | Review the requested effect and approve only through the supported policy flow; profile creation does not grant blanket execution. |

Offline tests cover profile creation/trust/expiry/catalog matching, fixed probe
accounting, receipt attribution and delayed-receipt completion. A live provider
walkthrough requires separately admitted spend, and clean-machine execution of
the final installed bytes remains a release qualification gate. These source
tests do not claim that either has already passed.

When finished with all setup and tasks in this shell, remove the process key:

```powershell
Remove-Item Env:\OPENROUTER_API_KEY -ErrorAction SilentlyContinue
```
