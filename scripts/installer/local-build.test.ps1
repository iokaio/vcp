# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
# Run the actual builder against inert compiler/build fixtures. No compilation,
# download or installed-product changes are performed.
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $temporaryBase ('vcp-local-build-tests-' + [guid]::NewGuid().ToString('N'))
$originalEncodedFlags = [Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')
function Check([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }
function Reject([scriptblock]$Action,[string]$Pattern) {
    $reason = $null
    try { & $Action | Out-Null } catch { $reason = $_.Exception.Message }
    Check ($reason -like $Pattern) "Expected $Pattern; actual: $reason"
}
try {
    foreach ($directory in 'scripts/release','release','artifacts/local-setup/0.2.6','artifacts/build-tools/inno-setup') {
        New-Item -ItemType Directory -Path (Join-Path $root $directory) -Force | Out-Null
    }
    $builder = Join-Path $root 'scripts/build-local-setup.ps1'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../build-local-setup.ps1') -Destination $builder
    $channelPath = Join-Path $root 'release/internal-beta.json'
    $compiler = Join-Path $root 'compiler.exe'
    [IO.File]::WriteAllText($compiler,'inert offline compiler fixture')
    $digest = (Get-FileHash -LiteralPath $compiler -Algorithm SHA256).Hash.ToLowerInvariant()
    $channel = @{native_version='0.2.6';installer=@{version='6.7.3';sha256=$digest;download_url='https://example.invalid/compiler.exe'}}
    $channel | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $channelPath -Encoding utf8NoBOM
    Copy-Item -LiteralPath $compiler -Destination (Join-Path $root ('artifacts/build-tools/inno-setup/innosetup-6.7.3-' + $digest + '.exe'))
    [IO.File]::WriteAllText((Join-Path $root 'artifacts/local-setup/0.2.6/preserved.txt'),'prior candidate')
    # The Node helper has its own source/provenance regression suite. This fixture
    # verifies the PowerShell orchestration consumes its result before compiling.
    @'
const fs = require('node:fs'), path = require('node:path');
if (process.argv[2] !== 'prepare') throw Error('Missing prepare operation');
const root = process.argv[3], output = process.argv[4];
const channelPath = path.join(root, 'release/internal-beta.json');
const channel = JSON.parse(fs.readFileSync(channelPath));
const version = '0.2.' + (Number(channel.native_version.split('.')[2]) + 1);
const versionRoot = path.join(output, version), canonical = path.join(root, 'artifacts/local-setup', version);
fs.mkdirSync(canonical);
if (versionRoot !== canonical) fs.mkdirSync(versionRoot, {recursive: true});
channel.native_version = version;
fs.writeFileSync(channelPath, JSON.stringify(channel));
console.log(JSON.stringify({version, versionRoot}));
'@ | Set-Content -LiteralPath (Join-Path $root 'scripts/release/local-version.cjs') -Encoding utf8NoBOM
    @'
param($OutputRoot,$Jobs)
if ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')) { throw 'Unexpected build override: CARGO_ENCODED_RUSTFLAGS' }
$repository = Split-Path -Parent $PSScriptRoot
$channel = Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json
if ((Split-Path -Leaf (Split-Path -Parent (Split-Path -Parent $OutputRoot))) -cne $channel.native_version) { throw 'Build used stale version or output path' }
$blocked = $false
try { $other = [IO.File]::Open((Join-Path $repository 'artifacts/local-setup-build.lock'),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None); $other.Dispose() }
catch [IO.IOException] { $blocked = $true }
if (-not $blocked) { throw 'Build did not retain the source version lock' }
Set-Content -LiteralPath (Join-Path $repository 'build-observed.txt') -Value $channel.native_version
Set-Content -LiteralPath (Join-Path $repository 'jobs-observed.txt') -Value $Jobs
throw 'Fixture stopped before production compilation'
'@ | Set-Content -LiteralPath (Join-Path $root 'scripts/build-production.ps1') -Encoding utf8NoBOM
    $retainedFlags = @('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31
    [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$retainedFlags,'Process')
    Reject { & $builder } '*Fixture stopped before production compilation*'
    Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq $retainedFlags) 'Local build changed retained caller flags'
    Check ((Get-Content -LiteralPath (Join-Path $root 'build-observed.txt') -Raw).Trim() -ceq '0.2.7') 'No-argument build did not use prepared version'
    $automaticJobs = [int](Get-Content -LiteralPath (Join-Path $root 'jobs-observed.txt') -Raw)
    Check ($automaticJobs -ge 1 -and $automaticJobs -le 32) 'Automatic build parallelism is outside the default budget'
    Check ($automaticJobs -lt [Environment]::ProcessorCount -or [Environment]::ProcessorCount -eq 1) 'Automatic build did not leave a logical processor free'
    Check ($automaticJobs -eq 1 -or ($automaticJobs * 4GB) -le [GC]::GetGCMemoryInfo().TotalAvailableMemoryBytes) 'Automatic build exceeded its memory budget'
    Check ((Get-Content -LiteralPath (Join-Path $root 'artifacts/local-setup/0.2.6/preserved.txt') -Raw) -ceq 'prior candidate') 'Old candidate changed'
    $lockPath = Join-Path $root 'artifacts/local-setup-build.lock'
    $handle = [IO.File]::Open($lockPath,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try {
        Reject { & $builder } '*Another local setup build*'
        Check ((Get-Content -LiteralPath $channelPath -Raw | ConvertFrom-Json).native_version -ceq '0.2.7') 'Concurrent build modified source'
    } finally { $handle.Dispose() }
    Reject { & $builder -NativeResult 'unused' } '*Supply NativeResult*'
    Reject { & $builder -NativeResult 'unused' -BuildReceipt 'unused' -Launcher 'unused' } '*already has an output directory*'
    Check ((Get-Content -LiteralPath $channelPath -Raw | ConvertFrom-Json).native_version -ceq '0.2.7') 'Receipt mode or invalid parameters bumped source version'
    $custom = Join-Path $root 'custom-output'
    Reject { & $builder -OutputRoot $custom } '*Fixture stopped before production compilation*'
    Check ((Get-Content -LiteralPath (Join-Path $root 'build-observed.txt') -Raw).Trim() -ceq '0.2.8') 'Retry did not use a new version'
    Check ((Test-Path -LiteralPath (Join-Path $root 'artifacts/local-setup/0.2.8')) -and (Test-Path -LiteralPath (Join-Path $custom '0.2.8'))) 'Custom output did not keep canonical reservation'
    Reject { & $builder -Jobs 23 } '*Fixture stopped before production compilation*'
    Check ([int](Get-Content -LiteralPath (Join-Path $root 'jobs-observed.txt') -Raw) -eq 23) 'Explicit parallelism was not passed to the production builder'
    foreach ($invalidJobs in 0,257) { Reject { & $builder -Jobs $invalidJobs } '*Cannot validate argument on parameter*' }
    Check ((Get-Content -LiteralPath $channelPath -Raw | ConvertFrom-Json).native_version -ceq '0.2.9') 'Invalid job counts consumed a candidate version'
    # Exercise the real production parameter binding and preflight guards only,
    # stopping before tool discovery, output allocation or compilation.
    $productionPath = Join-Path $PSScriptRoot '../build-production.ps1'
    $tokens=$null; $errors=$null
    $productionAst = [Management.Automation.Language.Parser]::ParseFile($productionPath,[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw $errors[0] }
    $toolAssignment = $productionAst.Find({ param($statement)
        $statement -is [Management.Automation.Language.AssignmentStatementAst] -and $statement.Left.Extent.Text -ceq '$releaseTool'
    },$true)
    if (-not $toolAssignment) { throw 'Production tool discovery boundary missing' }
    $guards = @($toolAssignment.Parent.Statements | Where-Object {
        $_ -is [Management.Automation.Language.IfStatementAst] -and $_.Extent.StartOffset -lt $toolAssignment.Extent.StartOffset
    })
    $preflight = [scriptblock]::Create("[CmdletBinding()]`n" + $productionAst.ParamBlock.Extent.Text + "`n" + ($guards.Extent.Text -join "`n") + "`nWrite-Output `$Jobs")
    Check ((& $preflight -Jobs 23) -eq 23) 'Production builder refused local parallelism'
    Check ((& $preflight -Jobs 256) -eq 256) 'Production builder refused the local upper boundary'
    Check ((& $preflight) -eq 2) 'Direct production build default changed'
    Check ((& $preflight -Release -ReviewedCommit ('a' * 40) -Jobs 16) -eq 16) 'Qualified release boundary changed'
    Reject { & $preflight -Release -ReviewedCommit ('a' * 40) -Jobs 23 } '*Qualified release builds require*'
    foreach ($invalidJobs in 0,257) { Reject { & $preflight -Jobs $invalidJobs } '*Cannot validate argument on parameter*' }
    $handle = [IO.File]::Open($lockPath,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    $handle.Dispose()
    Write-Output 'PASS local setup orchestration, version handoff, retry, receipt mode, exclusive lock and build parallelism'
} finally {
    $restoredFlags = if ($null -eq $originalEncodedFlags) { [NullString]::Value } else { $originalEncodedFlags }
    [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$restoredFlags,'Process')
    $resolved = [IO.Path]::GetFullPath($root)
    if ($resolved.StartsWith($temporaryBase,[StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-local-build-tests-*') {
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
