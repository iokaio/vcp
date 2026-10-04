#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Real metadata recovery and preflight with an inert CLI boundary: no network or inference.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-provider-adapter-tests-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check([bool]$Pass, [string]$Message) { if (-not $Pass) { throw $Message }; $script:checks++ }
function New-TestContext([string]$Mode) {
    $caseRoot = Join-Path $root $Mode
    New-Item -ItemType Directory -Path $caseRoot | Out-Null
    $catalog = Join-Path $caseRoot 'retained-endpoints.json'
    $snapshot = Join-Path $caseRoot 'retained-snapshot.json'
    Write-JsonFile $catalog @{ data = @{ id = 'fixture/model'; endpoints = @(@{ tag = 'fixture/endpoint' }) } }
    Write-JsonFile $snapshot @{
        # Unexpired evidence can still be rejected by a newer compiled adapter.
        valid_until = [DateTimeOffset]::UtcNow.AddHours(12).ToUnixTimeMilliseconds()
        raw_sha256 = (Get-FileHash -LiteralPath $catalog -Algorithm SHA256).Hash
        max_output = 8192
        compatibility = @{ id = 'openrouter-responses-adapter-contract/1/old'; model = 'fixture/model'; endpoint = 'fixture/endpoint' }
    }
    if ($Mode -in 'changed-retained-selection', 'changed-retained-endpoint-case') {
        # Keep the catalog/snapshot internally consistent while changing their
        # identity after the scenario selected fixture/model and fixture/endpoint.
        $retainedCatalog = Get-Content -LiteralPath $catalog -Raw | ConvertFrom-Json -AsHashtable
        $retainedSnapshot = Get-Content -LiteralPath $snapshot -Raw | ConvertFrom-Json -AsHashtable
        if ($Mode -eq 'changed-retained-selection') {
            $retainedCatalog.data.id = 'fixture/other-model'
            $retainedSnapshot.compatibility.model = 'fixture/other-model'
        }
        else {
            $retainedCatalog.data.endpoints[0].tag = 'fixture/ENDPOINT'
            $retainedSnapshot.compatibility.endpoint = 'fixture/ENDPOINT'
        }
        Write-JsonFile $catalog $retainedCatalog
        $retainedSnapshot.raw_sha256 = (Get-FileHash -LiteralPath $catalog -Algorithm SHA256).Hash
        Write-JsonFile $snapshot $retainedSnapshot
    }
    return @{
        Root = $caseRoot; Results = $caseRoot; Profiles = $caseRoot; Logs = $caseRoot
        Name = $Mode; RunId = 'test'; ProgressLog = Join-Path $caseRoot 'progress.log'
        Snapshot = $snapshot; Catalog = $catalog; SnapshotText = Get-Content -LiteralPath $snapshot -Raw
        Model = 'fixture/model'; Endpoint = 'fixture/endpoint'; OutputTokens = 8192; TurnBudgetUsd = [decimal]3
        Gates = [Collections.Generic.List[object]]::new(); Notes = [Collections.Generic.List[string]]::new()
    }
}
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    & $module {
        $script:adapterCalls = [Collections.Generic.List[object]]::new()
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, [string[]]$Arguments, [switch]$DenyProviderCredentials, $TimeoutSeconds, [switch]$NoGlobals)
            $script:adapterCalls.Add(@{ label = $Label; arguments = $Arguments; denied_credentials = [bool]$DenyProviderCredentials })
            $stdout = Join-Path $Ctx.Root ($Label + '.stdout')
            $stderr = Join-Path $Ctx.Root ($Label + '.stderr')
            $code = 0; $errorText = ''; $data = @{ status = 'ok'; model_calls = 0 }
            switch ($Label) {
                'version' { Write-Utf8File $stdout 'vcp fixture-version' }
                'inspect-bundle-help' { }
                'doctor' { }
                'credential-status' { }
                'skills-list' { }
                'models' { }
                'setup-profile' {
                    $code = 2
                    $errorText = if ($script:adapterMode -eq 'unrelated-error') { 'vcp: workspace trust is required' }
                        elseif ($script:adapterMode -eq 'fresh-window-error') { 'vcp: adapter contract or fresh metadata window does not match' }
                        else { 'vcp: provider capability unavailable: dated compatibility record; require a fresh matching catalog/snapshot' }
                }
                'current-adapter-metadata' {
                    if (($Arguments[0..1] -join ' ') -ne 'setup provider-metadata') { throw 'Metadata recovery used an unexpected CLI command' }
                    if ($script:adapterMode -eq 'unsupported-command') { $code = 2; $errorText = 'unrecognized subcommand provider-metadata'; break }
                    $output = $Arguments[([array]::IndexOf($Arguments, '--output') + 1)]
                    New-Item -ItemType Directory -Path $output | Out-Null
                    $endpoint = if ($script:adapterMode -eq 'wrong-endpoint') { 'fixture/other' }
                        elseif ($script:adapterMode -eq 'wrong-endpoint-case') { 'fixture/ENDPOINT' } else { 'fixture/endpoint' }
                    Write-JsonFile (Join-Path $output 'endpoints.json') @{ data = @{ id = 'fixture/model'; endpoints = @(@{ tag = $endpoint }) } }
                    $hash = (Get-FileHash -LiteralPath (Join-Path $output 'endpoints.json') -Algorithm SHA256).Hash
                    if ($script:adapterMode -eq 'wrong-hash') { $hash = '0' * 64 }
                    Write-JsonFile (Join-Path $output 'snapshot.json') @{
                        valid_until = [DateTimeOffset]::UtcNow.AddHours(24).ToUnixTimeMilliseconds(); raw_sha256 = $hash; max_output = 4096
                        compatibility = @{ id = 'openrouter-responses-adapter-contract/1/current'; model = 'fixture/model'; endpoint = $endpoint }
                    }
                    $data.status = 'created'
                }
                'setup-profile-current-adapter' {
                    if ($script:adapterMode -eq 'rejected-profile') { $code = 2; $errorText = 'vcp: provider capability unavailable: dated compatibility record'; break }
                    $snapshotPath = $Arguments[([array]::IndexOf($Arguments, '--snapshot') + 1)]
                    $catalogPath = $Arguments[([array]::IndexOf($Arguments, '--catalog') + 1)]
                    $profilePath = $Arguments[([array]::IndexOf($Arguments, '--output') + 1)]
                    $selected = Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json
                    if ($selected.compatibility.id -cne 'openrouter-responses-adapter-contract/1/current') { throw 'Retry used the stale adapter snapshot' }
                    Write-JsonFile $profilePath @{ provider = $selected; catalog = $catalogPath
                        budget_usd = $Arguments[([array]::IndexOf($Arguments, '--budget-usd') + 1)]
                        maximum_autonomy = $Arguments[([array]::IndexOf($Arguments, '--autonomy') + 1)] }
                }
                default { throw "Unexpected CLI invocation (inference prohibited): $Label" }
            }
            if (-not (Test-Path -LiteralPath $stdout)) { Write-Utf8File $stdout '{}' }
            Write-Utf8File $stderr $errorText
            return [pscustomobject]@{ ExitCode = $code; TimedOut = $false; InvalidLines = 0; Result = @{ data = $data }
                StdoutPath = $stdout; StderrPath = $stderr; Stderr = $errorText }
        }
    }
    foreach ($mode in 'success', 'fresh-window-error', 'unrelated-error', 'changed-retained-selection', 'changed-retained-endpoint-case', 'wrong-endpoint', 'wrong-endpoint-case', 'wrong-hash', 'unsupported-command', 'rejected-profile') {
        $ctx = New-TestContext $mode
        $priorSnapshot = $ctx.Snapshot; $priorCatalog = $ctx.Catalog
        $snapshotHash = (Get-FileHash -LiteralPath $priorSnapshot).Hash
        $catalogHash = (Get-FileHash -LiteralPath $priorCatalog).Hash
        & $module { param($mode) $script:adapterMode = $mode; $script:adapterCalls.Clear() } $mode
        Invoke-CommonPreflight $ctx 'src/tasks.ts'
        $calls = & $module { @($script:adapterCalls) }
        $metadata = @($calls | Where-Object label -eq 'current-adapter-metadata')
        $profiles = @($calls | Where-Object { $_.label -like 'setup-profile*' })
        $gate = @($ctx.Gates | Where-Object id -eq 'setup-profile')
        $success = $mode -in 'success', 'fresh-window-error'
        Check ($gate.Count -eq 1 -and $gate[0].required -and $gate[0].outcome -eq $(if ($success) { 'pass' } else { 'fail' })) "$mode produced the wrong setup gate outcome"
        Check ($snapshotHash -eq (Get-FileHash -LiteralPath $priorSnapshot).Hash -and $catalogHash -eq (Get-FileHash -LiteralPath $priorCatalog).Hash) "$mode changed retained source evidence"
        Check (@($calls | Where-Object { ($_.label -like 'setup-profile*' -or $_.label -eq 'current-adapter-metadata') -and -not $_.denied_credentials }).Count -eq 0) "$mode exposed provider credentials to metadata/profile preparation"
        Check ($ctx.Model -ceq 'fixture/model' -and $ctx.Endpoint -ceq 'fixture/endpoint') "$mode changed the configured provider identity"
        $expectedMetadata = if ($mode -in 'unrelated-error', 'changed-retained-selection', 'changed-retained-endpoint-case') { 0 } else { 1 }
        Check ($metadata.Count -eq $expectedMetadata) "$mode made an unexpected number of metadata requests"
        if ($mode -in 'changed-retained-selection', 'changed-retained-endpoint-case') {
            Check ($gate[0].detail -match 'Retained evidence changed the selected model or endpoint') "$mode did not report retained identity drift"
            Check (-not (Test-Path -LiteralPath (Join-Path $ctx.Results 'provider-adapter-update.json')) -and
                @(Get-ChildItem -LiteralPath $ctx.Root -Directory -Filter 'provider-current-*').Count -eq 0) "$mode created metadata output before validating the selected identity"
        }
        $expectedProfiles = if ($mode -in 'success', 'fresh-window-error', 'rejected-profile') { 2 } else { 1 }
        Check ($profiles.Count -eq $expectedProfiles) "$mode retried setup profile unexpectedly or more than once"
        foreach ($call in $profiles) {
            $arguments = $call.arguments
            Check (($arguments[0..1] -join ' ') -eq 'setup profile' -and $arguments -contains '--trust-workspace' -and
                $arguments[([array]::IndexOf($arguments, '--affected-path') + 1)] -eq 'src/tasks.ts' -and
                $arguments[([array]::IndexOf($arguments, '--budget-usd') + 1)] -eq '3.00' -and
                $arguments[([array]::IndexOf($arguments, '--autonomy') + 1)] -eq 'ask') "$mode changed profile request constraints"
        }
        if ($metadata.Count) {
            $arguments = $metadata[0].arguments
            Check ($arguments[([array]::IndexOf($arguments, '--model') + 1)] -ceq 'fixture/model' -and
                $arguments[([array]::IndexOf($arguments, '--endpoint') + 1)] -ceq 'fixture/endpoint') "$mode requested a different model or endpoint"
            $evidence = Get-Content -LiteralPath (Join-Path $ctx.Results 'provider-adapter-update.json') -Raw | ConvertFrom-Json
            Check ($evidence.model_calls -eq 0 -and $evidence.prior_snapshot -eq $priorSnapshot -and $evidence.prior_catalog -eq $priorCatalog) "$mode lost zero-inference provenance"
            $expectedStatus = if ($mode -in 'success', 'fresh-window-error', 'rejected-profile') { 'created' } else { 'failed' }
            Check ($evidence.status -eq $expectedStatus) "$mode failed to record the metadata recovery result"
        }
        if ($success) {
            $profile = Get-Content -LiteralPath (Join-Path $ctx.Profiles 'base-setup-profile-test.json') -Raw | ConvertFrom-Json
            Check ($ctx.Snapshot -ne $priorSnapshot -and $ctx.Catalog -ne $priorCatalog -and
                $profile.provider.compatibility.id -ceq 'openrouter-responses-adapter-contract/1/current') "$mode did not use current adapter evidence"
            Check ($profile.provider.compatibility.model -ceq 'fixture/model' -and $profile.provider.compatibility.endpoint -ceq 'fixture/endpoint') "$mode admitted a different provider identity"
            Check ($ctx.OutputTokens -eq 4096 -and $profile.maximum_autonomy -eq 'ask' -and $profile.budget_usd -eq '3.00') "$mode lost endpoint limits or profile constraints"
        }
        elseif ($mode -ne 'rejected-profile') {
            Check ($ctx.Snapshot -eq $priorSnapshot -and $ctx.Catalog -eq $priorCatalog) "$mode adopted failed metadata evidence"
        }
    }
    Write-Host "PASS: $checks offline provider adapter recovery checks"
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path $resolved -Leaf) -notlike 'vcp-provider-adapter-tests-*') { throw 'Unsafe test cleanup path' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
