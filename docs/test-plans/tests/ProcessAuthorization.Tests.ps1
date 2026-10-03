#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Offline profile-authorization boundary; no native VCP, credentials or inference.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
$module = Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $tempBase ('vcp-process-authorization-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }; $script:checks++
}
function New-Context([bool]$Allow = $false, [bool]$DryRun = $false) {
    $root = Join-Path $temporary ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $root | Out-Null
    return @{
        Workspace = $root; Profiles = $root; Results = $root; RunId = 'fixture'
        Catalog = (Join-Path $root 'endpoints.json'); SnapshotText = '{"fixture":"preserve"}'
        TurnBudgetUsd = [decimal]3; OutputTokens = 8192; MaxRequests = 96; DeadlineSeconds = 1800
        AllowProcessPublish = $Allow; SkipPaidStages = $DryRun
    }
}
function Read-Profile([string]$Path) { Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -Depth 100 }
$process = @{ name = 'node'; executable = 'C:/fixture/node.exe'; environment = @{}; max_timeout_ms = 900000 }
try {
    & $module {
        $script:attemptedCalls = 0
        function script:Invoke-Vcp { $script:attemptedCalls++; throw 'A profile permission check must not execute VCP.' }
    }
    $ctx = New-Context
    $errorText = $null
    try { New-ScenarioProfile $ctx 'full-default' @('src') -Processes @($process) | Out-Null } catch { $errorText = $_.Exception.Message }
    Check ($errorText -like '*explicit permission*AllowProcessPublish*') 'Full mode did not explain required process permission'
    Check (-not (Test-Path -LiteralPath (Join-Path $ctx.Profiles 'full-default-fixture.json'))) 'Full mode created an unauthorized execution profile'
    Check ((& $module { $script:attemptedCalls }) -eq 0) 'Default refusal happened after VCP execution'
    $decision = Read-Profile (Join-Path $ctx.Results 'process-authorization.json')
    Check (-not $decision.allow_process_publish -and $decision.profiles[0].decision -like 'refused*') 'Default refusal was not retained'
    Check ($decision.reason -like '*every process*' -and $decision.scope -like '*this scenario run*') 'Authorization record omits scope or effect-classification explanation'

    $ctx = New-Context -Allow $true
    $profile = Read-Profile (New-ScenarioProfile $ctx 'execution' @('src') -Processes @($process))
    foreach ($effect in 'read', 'write', 'execute', 'network', 'install', 'publish', 'opaque') {
        Check ($profile.automatic_effects -contains $effect) "Explicit execution permission did not cover $effect"
    }
    Check ($profile.automatic_effects.Count -eq 7) 'Explicit permission duplicated or added unrelated effects'
    Check ($profile.processes.Count -eq 1 -and $profile.processes[0].executable -eq $process.executable) 'Authorization changed registered processes'
    Check ($profile.deadline_seconds -eq 1800 -and $profile.provider_timeout_seconds -eq 300) 'Authorization shortened task or transport deadlines'
    Check ($profile.provider.fixture -eq 'preserve') 'Authorization changed provider input'
    $decision = Read-Profile (Join-Path $ctx.Results 'process-authorization.json')
    Check ($decision.allow_process_publish -and $decision.profiles[0].decision -like '*explicit AllowProcessPublish*') 'Explicit consent was not retained'
    Check ($decision.profiles[0].processes[0] -eq 'node') 'Consent did not identify the registered process scope'

    $review = Read-Profile (New-ScenarioProfile $ctx 'review' @('src') -MaximumAutonomy 'plan' -AutomaticEffects @('read'))
    Check (($review.automatic_effects -join ',') -eq 'read' -and $review.maximum_autonomy -eq 'plan') 'Opt-in broadened review authority'
    $guardrail = Read-Profile (New-ScenarioProfile $ctx 'guardrail' @('src') -Processes @($process) -Guardrail)
    Check ($guardrail.automatic_effects -notcontains 'publish' -and $guardrail.processes.Count -eq 1) 'Opt-in broadened guardrail authority'
    $untrusted = Read-Profile (New-ScenarioProfile $ctx 'untrusted' @('src') -Processes @($process) -TrustWorkspace $false -Guardrail)
    Check (-not $untrusted.trust_workspace -and $untrusted.automatic_effects -notcontains 'publish') 'Opt-in changed untrusted-workspace guardrail'
    $nonProcess = Read-Profile (New-ScenarioProfile $ctx 'non-process' @('src'))
    Check ($nonProcess.automatic_effects -notcontains 'publish') 'Opt-in broadened profiles without processes'

    $ctx = New-Context
    $readOnly = Read-Profile (New-ScenarioProfile $ctx 'read-only' @('src') -Processes @($process) -MaximumAutonomy 'plan' -AutomaticEffects @('read'))
    Check (($readOnly.automatic_effects -join ',') -eq 'read') 'Default permission prevented a read-only profile'
    $guardrail = Read-Profile (New-ScenarioProfile $ctx 'default-guardrail' @('src') -Processes @($process) -Guardrail)
    Check ($guardrail.automatic_effects -notcontains 'publish') 'Default guardrail unexpectedly required or gained permission'
    $ctx = New-Context -DryRun $true
    $dry = Read-Profile (New-ScenarioProfile $ctx 'dry-run' @('src') -Processes @($process))
    Check ($dry.automatic_effects -notcontains 'publish') 'DryRun silently authorized publication'
    $decision = Read-Profile (Join-Path $ctx.Results 'process-authorization.json')
    Check ($decision.dry_run -and -not $decision.allow_process_publish -and $decision.profiles[0].decision -like '*required before Full*') 'DryRun did not preserve the pending Full permission decision'

    # Direct-script users get the same explicit switch and forwarding semantics.
    foreach ($file in Get-ChildItem -LiteralPath $scenarioRoot -File -Filter 'scenario-*.ps1') {
        $tokens = $null; $errors = $null
        $ast = [Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors)
        Check ($errors.Count -eq 0) "Scenario parse failure: $($file.Name)"
        $parameter = @($ast.ParamBlock.Parameters | Where-Object { $_.Name.VariablePath.UserPath -eq 'AllowProcessPublish' })
        Check ($parameter.Count -eq 1 -and $null -eq $parameter[0].DefaultValue) "$($file.Name) lacks an explicit-only authorization switch"
        $init = $ast.Find({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Initialize-VcpScenario' }, $true)
        $forwarded = @($init.CommandElements | Where-Object { $_ -is [Management.Automation.Language.CommandParameterAst] -and $_.ParameterName -eq 'AllowProcessPublish' })
        Check ($forwarded.Count -eq 1 -and $forwarded[0].Argument.Extent.Text -eq '$AllowProcessPublish') "$($file.Name) did not forward the selected permission"
    }
    Check ((& $module { $script:attemptedCalls }) -eq 0) 'Profile tests executed VCP'
    Write-Host "Process authorization regressions passed: $checks checks; no VCP or inference executed."
}
finally {
    Remove-Module $module -Force
    $resolved = [IO.Path]::GetFullPath($temporary)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $resolved -Leaf) -notlike 'vcp-process-authorization-tests-*') { throw 'Unsafe cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
