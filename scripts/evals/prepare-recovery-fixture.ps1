# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Developer synthetic input preparation, never a production recovery result.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$TestExecutable,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedSha256,
    [Parameter(Mandatory)][string]$GitExecutable,
    [Parameter(Mandatory)][string]$OutputRoot,
    [string[]]$SyncRoots = @(),
    [ValidateRange(1,900)][int]$DeadlineSeconds = 300
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows fixture preparation required' }
. (Join-Path $PSScriptRoot '../release/candidate-runtime.ps1')
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Require([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }
function PlainAncestors([string]$Path) {
    for ($item=[IO.DirectoryInfo]::new($Path); $null -ne $item; $item=$item.Parent) {
        if (Test-Path -LiteralPath $item.FullName) {
            $entry=Get-Item -LiteralPath $item.FullName -Force
            Require ($entry.PSIsContainer -and -not ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Private fixture root has redirected or non-directory ancestors'
        }
        Require (-not (Test-Path -LiteralPath (Join-Path $item.FullName '.git'))) 'Private fixture root must be outside repository trees'
    }
}
Require ([IO.Path]::IsPathFullyQualified($OutputRoot)) 'An absolute fresh private output root is required'
$root=[IO.Path]::GetFullPath($OutputRoot).TrimEnd('\','/')
PlainAncestors $root
Require (-not (Test-Path -LiteralPath $root)) 'Fixture preparation never reuses an existing output directory'
$knownSync=@($SyncRoots)
foreach ($name in @('OneDrive','OneDriveConsumer','OneDriveCommercial')) {
    $value=[Environment]::GetEnvironmentVariable($name); if ($value) { $knownSync += $value }
}
foreach ($sync in $knownSync) {
    $selected=[IO.Path]::GetFullPath($sync).TrimEnd('\','/')
    Require (-not ($root.Equals($selected,[StringComparison]::OrdinalIgnoreCase) -or $root.StartsWith($selected+'\',[StringComparison]::OrdinalIgnoreCase) -or $selected.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase))) 'Fixture root overlaps a declared or known sync root'
}
$test=(Resolve-Path -LiteralPath $TestExecutable).Path
$git=(Resolve-Path -LiteralPath $GitExecutable).Path
Require ((Hash $test) -ceq $ExpectedSha256) 'Selected fixture test executable hash differs'
Require ((Get-Item -LiteralPath $test).Name -cmatch '^canonical_host-[a-f0-9]+\.exe$') 'Select the compiled vcp-lifecycle canonical_host integration test executable'
$testName='portable_operator_fixture::export_operator_handoff_fixture_with_child_claim_and_real_accounting'
New-Item -ItemType Directory -Path $root | Out-Null
$inputs=Join-Path $root 'recovery-inputs'
$temporary=Join-Path $root 'temporary'
New-Item -ItemType Directory -Path $temporary | Out-Null
$report=[ordered]@{
    schema='vcp-synthetic-recovery-fixture/1'; status='running'; synthetic=$true
    generator_test=$testName; generator_sha256=$ExpectedSha256; git_sha256=(Hash $git)
    script_sha256=(Hash $PSCommandPath); created_at=[DateTime]::UtcNow.ToString('o'); fixtures=@()
    provider_requests=0; production_recovery='not_run'; independent_machine_recovery='not_run'
    private_contents='Recovery keys, ciphertext, temporary histories and raw process logs remain private; copy only this reviewed receipt.'
}
try {
    # All test temp state and the generator's sibling transfer/Recovery copy stay
    # inside this newly created private root. Ambient provider credentials and
    # proxy settings are absent from the shared bounded process supervisor.
    $environment=@{VCP_TEST_GIT=$git;VCP_TEST_U04_EXPORT=$inputs;TEMP=$temporary;TMP=$temporary;CODEX_TEST_ENVIRONMENT='local';RUST_MIN_STACK='16777216'}
    $listing=Invoke-BetaProcess $test @('--list','--format','terse') $root $environment 30
    Require ($listing.stdout -match ('(?m)^'+[regex]::Escape($testName)+': test\r?$')) 'Selected integration test is missing the exact fixture generator'
    $result=Invoke-BetaProcess $test @($testName,'--exact','--ignored','--test-threads=1') $root $environment $DeadlineSeconds
    Require ($result.stdout -match 'test result: ok\. 1 passed; 0 failed; 0 ignored;') 'Exactly one synthetic fixture test must pass'
    $keyHashes=@()
    foreach ($backend in @('files','sqlite')) {
        $seed=Join-Path $inputs $backend
        foreach ($entry in Get-ChildItem -LiteralPath $seed -Recurse -Force) {
            Require (-not ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Generated fixture contains a redirected path'
        }
        $manifest=Join-Path $seed 'fixture.json'; $ciphertext=Join-Path $seed 'snapshot.age'
        Require ((Get-Item -LiteralPath $manifest).Length -le 1MB) 'Fixture metadata exceeds the explicit bound'
        $fixture=Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
        Require ($fixture.provider_requests -eq 0 -and $fixture.attempts -eq 2 -and $fixture.settled -ceq '50' -and $fixture.unresolved -ceq '67') 'Synthetic fixture accounting or no-provider contract differs'
        Require ($fixture.workspace -and $fixture.root_task -and $fixture.child_task -and $fixture.root_task -cne $fixture.child_task) 'Distinct retained root and child tasks required'
        Require (@($fixture.claims).Count -gt 0 -and @($fixture.claim_records).Count -gt 0) 'Synthetic fixture must retain memory claims'
        Require ($fixture.lineage -ceq ('f'*64) -and $fixture.checkpoint.sequence -eq 0 -and $fixture.checkpoint.deletion -eq 0 -and $null -eq $fixture.checkpoint.parent) 'Independent enrollment checkpoint differs'
        $expected=@{'tracked.txt'="unstaged`n";'untracked.txt'="required untracked bytes`n";'source.rs'="pub fn retained_answer() -> u32 { 42 }`n"}
        Require (@($fixture.expected_files.PSObject.Properties).Count -eq $expected.Count) 'Synthetic source inventory differs'
        foreach ($entry in $expected.GetEnumerator()) { Require ($fixture.expected_files.($entry.Key) -ceq $entry.Value) 'Synthetic source bytes differ' }
        $cipher=(Get-Item -LiteralPath $ciphertext)
        Require ($cipher.Length -gt 0 -and $cipher.Length -le 64MB -and $cipher.Length -eq $fixture.bytes -and (Hash $ciphertext) -ceq $fixture.ciphertext_sha256) 'Encrypted fixture hash or bounded length differs'
        $keys=@(Get-ChildItem -LiteralPath (Join-Path $seed 'recovery') -File -Filter '*.recovery')
        Require ($keys.Count -eq 1 -and $keys[0].Length -gt 0 -and $keys[0].Length -le 16KB) 'Exactly one bounded private recovery copy required'
        $keyHash=Hash $keys[0].FullName
        $keyHashes += $keyHash
        $report.fixtures += @{backend=$backend;fixture_sha256=(Hash $manifest);ciphertext_sha256=$fixture.ciphertext_sha256;ciphertext_bytes=$cipher.Length;recovery_copy_sha256=$keyHash;root_task=$fixture.root_task;child_task=$fixture.child_task;settled='50';unresolved='67';attempts=2}
    }
    # The existing fixture deliberately uses the same synthetic workspace label
    # in isolated data roots. Independent keys provide the wrong-key control.
    Require ($keyHashes[0] -cne $keyHashes[1]) 'Backends require independent keys for negative recovery controls'
    Require ((Hash $test) -ceq $ExpectedSha256) 'Fixture generator changed during execution'
    $report.status='pass'
} catch {
    $report.status='fail'
    # Test panic output is retained only in the private supervisor logs. It may
    # describe private fixture paths; never echo it into a public build log.
    throw 'Synthetic fixture preparation failed; inspect the private root and its process logs. No production recovery result was produced.'
} finally {
    $report.completed_at=[DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-Path $root 'fixture-preparation.json') -Encoding utf8
}
$report | ConvertTo-Json -Depth 15
