# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
# Exercise compiler acquisition without network access or building a candidate.
$tokens = $null; $errors = $null
$builder = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../build-local-setup.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw $errors[0] }
foreach ($name in 'Ordinary-File','Hash','Resolve-CompilerInstaller') {
    $definition = $builder.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name},$true)
    . ([scriptblock]::Create($definition.Extent.Text))
}
$parameter = $builder.ParamBlock.Parameters | Where-Object { $_.Name.VariablePath.UserPath -ceq 'CompilerInstaller' }
if ($parameter.Extent.Text -match 'Mandatory') { throw 'Default compiler still requires an interactive parameter prompt' }
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $temporaryBase ('vcp-compiler-tests-' + [guid]::NewGuid().ToString('N'))
$script:downloads = 0
$script:mode = 'success'
$script:compilerBytes = [Text.Encoding]::UTF8.GetBytes('offline pinned compiler fixture')
function Invoke-WebRequest {
    param($Uri,$OutFile,$TimeoutSec)
    $script:downloads++
    if ($Uri.AbsoluteUri -cne 'https://example.invalid/compiler.exe' -or $TimeoutSec -le 0) { throw 'Unpinned download or missing timeout' }
    if ($script:mode -eq 'success') { [IO.File]::WriteAllBytes($OutFile,$script:compilerBytes) }
    else { [IO.File]::WriteAllText($OutFile,'wrong or partial bytes') }
    if ($script:mode -eq 'failure') { throw 'fixture network failure' }
}
function Check([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }
function Reject([scriptblock]$Action,[string]$Pattern) {
    $reason = $null
    try { & $Action | Out-Null } catch { $reason = $_.Exception.Message }
    Check ($reason -like $Pattern) "Expected $Pattern; actual: $reason"
}
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $explicit = Join-Path $root 'downloaded.exe'
    [IO.File]::WriteAllBytes($explicit,$script:compilerBytes)
    $pin = @{version='6.7.3';sha256=(Hash $explicit);download_url='https://example.invalid/compiler.exe'}
    $cache = Join-Path $root 'cache'
    Check ((Resolve-CompilerInstaller $explicit $pin $cache) -ceq $explicit) 'Explicit compiler path changed'
    Check ($script:downloads -eq 0 -and -not (Test-Path -LiteralPath $cache)) 'Explicit offline compiler caused acquisition'
    $wrong = Join-Path $root 'wrong.exe'
    [IO.File]::WriteAllText($wrong,'wrong bytes')
    Reject { Resolve-CompilerInstaller $wrong $pin $cache } '*hash mismatch*'
    Check ($script:downloads -eq 0) 'Invalid explicit compiler triggered a fallback download'
    $resolved = Resolve-CompilerInstaller '' $pin $cache
    Check ((Hash $resolved) -ceq $pin.sha256 -and $script:downloads -eq 1) 'Default compiler was not downloaded and verified'
    Check ((Resolve-CompilerInstaller '' $pin $cache) -ceq $resolved -and $script:downloads -eq 1) 'Verified cache was not reused offline'
    [IO.File]::WriteAllText($resolved,'tampered cache')
    Reject { Resolve-CompilerInstaller '' $pin $cache } '*hash mismatch*'
    Check ($script:downloads -eq 1) 'Tampered cache was overwritten'
    foreach ($mode in 'bad-hash','failure') {
        $script:mode = $mode
        $failedCache = Join-Path $root $mode
        $pattern = if ($mode -eq 'failure') { '*network failure*' } else { '*hash mismatch*' }
        Reject { Resolve-CompilerInstaller '' $pin $failedCache } $pattern
        Check (@(Get-ChildItem -LiteralPath $failedCache -Force).Count -eq 0) 'Unverified or partial download remained in the cache'
    }
    $script:mode = 'success'
    $retry = Resolve-CompilerInstaller '' $pin (Join-Path $root 'failure')
    Check ((Hash $retry) -ceq $pin.sha256) 'A failed download prevented a verified retry'
    $pin.download_url = 'http://example.invalid/compiler.exe'
    $before = $script:downloads
    Reject { Resolve-CompilerInstaller '' $pin (Join-Path $root 'http') } '*HTTPS*'
    Check ($script:downloads -eq $before) 'Insecure compiler URL caused a download'
    $blocked = Join-Path $root 'cache-file'
    [IO.File]::WriteAllText($blocked,'existing file')
    Reject { Resolve-CompilerInstaller '' $pin $blocked } '*cache directory*'
    Write-Output 'PASS: default acquisition, pinned hashes, offline override/cache, failure cleanup and retry, HTTPS and cache-path validation.'
} finally {
    $full = [IO.Path]::GetFullPath($root)
    if (-not $full.StartsWith($temporaryBase.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-compiler-tests-*') { throw 'Unsafe test cleanup path' }
    if (Test-Path -LiteralPath $full) { Remove-Item -LiteralPath $full -Recurse -Force }
}
