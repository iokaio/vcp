#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Shared project lifecycle regressions: no VCP execution, network, or inference.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -DisableNameChecking -PassThru
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-project-context-' + [guid]::NewGuid().ToString('N'))
$checks = 0
$ownedTranscript = $false
$junction = $null
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function Check-Rejected([scriptblock]$Action, [string]$Pattern, [string]$Message) {
    $caught = $null
    try { & $Action | Out-Null } catch { $caught = $_.Exception.Message }
    Check ($null -ne $caught -and $caught -like $Pattern) "$Message (actual: $caught)"
}
function New-Context([string]$Project, [string]$Runs, [string]$Name = 'fixture') {
    $context = Initialize-VcpScenario -Name $Name -RunRoot $Runs -ProjectPath $Project `
        -ProviderGeneration $generation -Vcp $pwsh -SkipPaidStages
    $script:ownedTranscript = $true
    try { return $context }
    finally {
        Stop-Transcript | Out-Null
        $script:ownedTranscript = $false
        $context.Transcript = $false
    }
}
function Invoke-FixtureGit([string[]]$Arguments) {
    $output = & $git -C $project @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Fixture git failed: $output" }
}

try {
    $pwsh = (Get-Process -Id $PID).Path
    $git = (Get-Command git -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    $generation = Join-Path $testRoot 'configured-provider'
    $project = Join-Path $testRoot 'project'
    $runs = Join-Path $testRoot 'runs'
    $snapshot = @{
        valid_until = [DateTimeOffset]::UtcNow.AddHours(1).ToUnixTimeMilliseconds()
        compatibility = @{ model = 'offline/model'; endpoint = 'offline/endpoint' }
        max_output = 4096
    }
    Write-SeedFiles -Root $generation -Files @{
        'snapshot.json' = ($snapshot | ConvertTo-Json -Depth 5)
        'endpoints.json' = '{}'
    }
    $contexts = @()
    foreach ($name in 'a-vue-taskboard', 'b-aspnet-inventory', 'c-java-ledger-cli', 'd-python-textlab') {
        $separate = New-Context (Join-Path $testRoot "projects/$name") $runs $name
        $contexts += $separate
        $arguments = & $module { param($context) Get-VcpGlobalArguments $context } $separate
        Check ($arguments[([array]::IndexOf($arguments, '--workspace') + 1)] -eq $separate.Workspace) "$name CLI workspace binding changed."
        Check ($arguments[([array]::IndexOf($arguments, '--data-dir') + 1)] -eq $separate.Data) "$name CLI data binding changed."
        $profile = New-ScenarioProfile $separate 'independent-review' @('README.md') -MaximumAutonomy 'plan' -AutomaticEffects @('read')
        $document = Get-Content -LiteralPath $profile -Raw | ConvertFrom-Json
        Check ($document.workspace -eq $separate.Workspace) "$name profile uses another project's workspace."
        Check (-not $separate.ReuseProject) "$name depends on an earlier scenario's project."
    }
    foreach ($key in 'Workspace', 'Data', 'Profiles', 'Logs', 'Results') {
        Check (@($contexts | ForEach-Object { $_[$key] } | Select-Object -Unique).Count -eq 4) "Scenarios share $key."
    }
    $checkout = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
    Check-Rejected { New-Context $checkout $runs } '*outside the VCP source checkout*' 'VCP checkout must not become a scenario project.'
    Check-Rejected { New-Context (Join-Path $checkout 'scenario-fixture') $runs } '*outside the VCP source checkout*' 'A child of the VCP checkout must not become a scenario project.'
    Check (-not (Test-Path -LiteralPath $project)) 'Fixture project must initially be absent.'
    $created = New-Context $project $runs
    Check (Test-Path -LiteralPath $project -PathType Container) 'Initialization must create a missing project directory.'
    Check (-not $created.ReuseProject) 'A newly created project must not be marked reused.'
    Check ($created.Workspace -eq [IO.Path]::GetFullPath($project)) 'Explicit ProjectPath was not selected.'
    Check ($created.Snapshot -eq (Join-Path $generation 'snapshot.json')) 'Bare configured-provider snapshot was not used.'
    Check ($created.OutputTokens -eq 4096) 'Configured provider output limit was not honored.'
    foreach ($key in 'Data', 'Logs', 'Profiles', 'Results') {
        Check (Test-Path -LiteralPath $created[$key] -PathType Container) "$key directory was not created."
        Check (-not $created[$key].StartsWith($project.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) "$key must stay outside the selected project."
    }
    Check (Test-Path -LiteralPath (Join-Path $created.Logs 'console-transcript.log')) 'Transcript must be available in run logs.'

    # Exercise overlap validation before creating a Git repository, which has its own earlier rejection.
    Check-Rejected { New-Context $project (Join-Path $project 'run-output') } '*must be outside the selected project*' 'Run evidence nested in the project must be rejected.'
    $filePath = Join-Path $testRoot 'not-a-directory'
    Write-Utf8File $filePath 'sentinel'
    Check-Rejected { New-Context $filePath $runs } '*ProjectPath must identify a directory*' 'A file ProjectPath must be rejected.'

    Write-SeedFiles -Root $project -Files @{ 'sentinel.txt' = 'staged user content'; 'untracked.txt' = 'untracked user content' }
    Invoke-FixtureGit @('init', '-q', '-b', 'main')
    Check-Rejected { New-Context (Join-Path $project 'nested-scenario') $runs } '*nested in another git project*' 'Scenarios must not share a containing Git project.'
    Check (-not (Test-Path -LiteralPath (Join-Path $project 'nested-scenario'))) 'Rejected nested project was created.'
    Invoke-FixtureGit @('add', '--', 'sentinel.txt')
    Write-Utf8File (Join-Path $project 'sentinel.txt') 'unstaged user content'
    $indexPath = Join-Path $project '.git/index'
    $headPath = Join-Path $project '.git/HEAD'
    $indexHash = Get-Sha256 $indexPath
    $headHash = Get-Sha256 $headPath
    $sentinelHash = Get-Sha256 (Join-Path $project 'sentinel.txt')
    $reused = New-Context $project $runs
    Check $reused.ReuseProject 'A populated project must be marked reused.'
    Check ($reused.Workspace -eq $created.Workspace) 'Repeat initialization must use the same project.'
    Check ($reused.Root -ne $created.Root) 'Repeat execution must allocate separate evidence directories.'
    Initialize-GitCheckpoint $reused
    # Even a pre-populated Git field must not enable checkpoint writes on a reused project.
    $reused.Git = $git
    Save-Checkpoint $reused 'must not commit user changes'
    Check ((Get-Sha256 $indexPath) -eq $indexHash) 'Reusing a project or checkpointing changed the Git index.'
    Check ((Get-Sha256 $headPath) -eq $headHash) 'Reusing a project or checkpointing changed Git HEAD.'
    Check (-not (Test-Path -LiteralPath (Join-Path $project '.git/refs/heads/main'))) 'Reusing an unborn repository must not create a commit.'
    Check ((Get-Sha256 (Join-Path $project 'sentinel.txt')) -eq $sentinelHash) 'Existing unstaged content was changed.'
    Check ((Get-Content -LiteralPath (Join-Path $project 'untracked.txt') -Raw) -eq 'untracked user content') 'Existing untracked content was changed.'

    $plain = Join-Path $testRoot 'plain-project'
    Write-SeedFiles -Root $plain -Files @{ 'source.txt' = 'plain existing project' }
    $plainContext = New-Context $plain $runs
    Initialize-GitCheckpoint $plainContext
    Save-Checkpoint $plainContext 'must not initialize Git'
    Check (-not (Test-Path -LiteralPath (Join-Path $plain '.git'))) 'An existing non-Git project must not acquire a repository.'

    Write-SeedFiles -Root $project -Files @{ 'sentinel.txt' = 'replacement'; 'new/nested.txt' = 'new content' } -MissingOnly
    Check ((Get-Sha256 (Join-Path $project 'sentinel.txt')) -eq $sentinelHash) 'MissingOnly overwrote an existing source file.'
    Check ((Get-Content -LiteralPath (Join-Path $project 'new/nested.txt') -Raw) -eq 'new content') 'MissingOnly failed to create a missing nested file.'
    Check-Rejected { Write-SeedFiles -Root $project -Files @{ '../escape.txt' = 'escape' } -MissingOnly } '*must stay inside*' 'MissingOnly must reject traversal.'
    Check (-not (Test-Path -LiteralPath (Join-Path $testRoot 'escape.txt'))) 'Traversal wrote outside the project.'
    Check-Rejected { Write-SeedFiles -Root $project -Files @{ '../not-a-directory' = 'escape' } -MissingOnly } '*must stay inside*' 'MissingOnly must reject traversal even when the target exists.'

    $outside = Join-Path $testRoot 'junction-target'
    New-Item -ItemType Directory -Path $outside | Out-Null
    $junction = Join-Path $project 'linked'
    New-Item -ItemType Junction -Path $junction -Target $outside | Out-Null
    Check-Rejected { New-Context $junction $runs } '*link or junction*' 'A project alias must not bypass project separation.'
    Check-Rejected { New-Context (Join-Path $junction 'new-project') $runs } '*link or junction*' 'A junction ancestor must not bypass project separation.'
    Check (-not (Test-Path -LiteralPath (Join-Path $outside 'new-project'))) 'Project validation wrote through a junction.'
    Check-Rejected { Write-SeedFiles -Root $project -Files @{ 'linked/escape.txt' = 'escape' } -MissingOnly } '*link or junction*' 'MissingOnly must reject a junction parent.'
    Check (-not (Test-Path -LiteralPath (Join-Path $outside 'escape.txt'))) 'Junction traversal wrote outside the project.'
    Write-Host "Project context regressions passed ($checks checks; no VCP calls)."
}
finally {
    if ($ownedTranscript) { Stop-Transcript | Out-Null }
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing cleanup outside the temporary test root.' }
    if ($junction -and (Test-Path -LiteralPath $junction)) {
        $resolvedJunction = [IO.Path]::GetFullPath($junction)
        if (-not $resolvedJunction.StartsWith($resolved.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing cleanup of an unbounded junction.' }
        # Delete the junction itself before recursively removing the fixture root.
        Remove-Item -LiteralPath $resolvedJunction -Force
    }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
