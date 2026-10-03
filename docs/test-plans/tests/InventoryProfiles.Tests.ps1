#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise B's actual profile and prompt composition without processes or inference.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$source = Get-Content -LiteralPath (Join-Path $scenarioRoot 'scenario-b-aspnet-inventory.ps1') -Raw
$start = $source.IndexOf("    `$stage = 'P1-profiles'")
$end = $source.IndexOf('    foreach ($pair in @(@(''main'', $profileMain)', $start)
Assert-That ($start -ge 0 -and $end -gt $start) 'Inventory profile block missing'
$compose = [scriptblock]::Create($source.Substring($start, $end - $start))
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
Assert-That ($errors.Count -eq 0) 'Inventory scenario parse failure'
$assignment = $ast.Find({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$environmentBlock' }, $true)
$environmentBlock = $assignment.Right.Expression.Value
$promptFunction = $ast.Find({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'New-Prompt' }, $true)
. ([scriptblock]::Create($promptFunction.Extent.Text))
function Get-Solution { Join-Path $ws 'Inventory.sln' }
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-inventory-profiles-' + [guid]::NewGuid().ToString('N'))
$dotnet = (Get-Process -Id $PID).Path
$major = 10; $tfm = 'net10.0'; $efVersion = '10.0.0'
try {
    foreach ($reuse in @($false, $true)) {
        $ws = Join-Path $root ([string]$reuse)
        $ctx = @{
            Workspace = $ws; Profiles = $ws; Temp = $ws; Env = (Join-Path $ws 'env'); Results = $ws; RunId = 'fixture'
            AllowProcessPublish = $true; ReuseProject = $reuse
            Catalog = (Join-Path $ws 'endpoints.json'); SnapshotText = '{}'
            TurnBudgetUsd = [decimal]3; OutputTokens = 8192; MaxRequests = 96
            DeadlineSeconds = 1800; ShortDeadlineSeconds = 150
        }
        $settings = Join-Path $ws 'src/Inventory.Web/appsettings.Development.json'
        Write-Utf8File $settings '{"ConnectionStrings":{"Inventory":"Server=old;Database=preserve-me;Integrated Security=True"}}'
        $before = Get-Sha256 $settings
        . $compose
        foreach ($path in @($profileMain, $profileShort, $profileUntrusted)) {
            $profile = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -Depth 100
            Assert-That ($profile.processes.Count -eq 1) 'Inventory lost its process'
            $process = $profile.processes[0]
            # Runtime public-environment contract; scenario-specific settings are not bootstrap values.
            $allowed = @('SYSTEMROOT', 'WINDIR', 'PATH', 'PATHEXT', 'TEMP', 'TMP', 'LANG', 'LC_ALL', 'TERM', 'CI', 'RUST_BACKTRACE', 'CARGO_TARGET_DIR', 'CARGO_HOME', 'LIB', 'INCLUDE', 'LIBPATH', 'PROGRAMFILES', 'PROGRAMFILES(X86)', 'APPDATA', 'LOCALAPPDATA', 'DOTNET_CLI_HOME')
            foreach ($key in $process.environment.PSObject.Properties.Name) {
                Assert-That ($allowed -contains $key) "Unsupported process environment: $key"
            }
            Assert-That ($process.executable -eq $dotnet -and $process.max_timeout_ms -eq 1200000) 'Inventory process contract changed'
            foreach ($key in 'APPDATA', 'LOCALAPPDATA', 'DOTNET_CLI_HOME') {
                $path = $process.environment.$key
                Assert-That ($path.StartsWith($ctx.Env + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) "$key inherited account state"
                Assert-That (Test-Path -LiteralPath $path -PathType Container) "$key bootstrap directory was not created"
            }
        }
        $review = Get-Content -LiteralPath $profileReview -Raw | ConvertFrom-Json -Depth 100
        Assert-That ($review.processes.Count -eq 0 -and ($review.automatic_effects -join ',') -eq 'read') 'Review gained execution permissions'
        foreach ($connection in @('Server=(localdb)\MSSQLLocalDB;Database=VcpInventory_current;Trusted_Connection=True', 'Server="host with spaces";Database=VcpInventory_current;Integrated Security=True')) {
            $prompt = New-Prompt 'Task body'
            $match = [regex]::Match($prompt, '(?s)`(\["tool".*?\])`')
            Assert-That $match.Success 'EF argument example missing'
            $arguments = $match.Groups[1].Value | ConvertFrom-Json
            $separator = [array]::IndexOf($arguments, '--')
            Assert-That ($separator -gt 0) 'EF application arguments separator missing'
            Assert-That ($arguments[$separator + 1] -eq '--environment' -and $arguments[$separator + 2] -eq 'Development') 'Development environment not forwarded'
            Assert-That ($arguments[$separator + 3] -eq '--ConnectionStrings:Inventory' -and $arguments[$separator + 4] -ceq $connection) 'Run connection did not survive JSON argument encoding'
            Assert-That ($arguments.Count -eq $separator + 5) 'Connection split into extra arguments'
            Assert-That ($prompt -notmatch '\{\{[A-Z_]+\}\}') 'Unresolved prompt placeholder'
        }
        Assert-That ((Get-Sha256 $settings) -eq $before) 'Profile/prompt preparation changed protected appsettings'
    }
    Write-Host 'Inventory profile regressions passed: allowed environment, read-only review, fresh/reused projects, literal EF overrides, protected configuration preserved.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-inventory-profiles-*') { throw 'Unsafe cleanup path' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
