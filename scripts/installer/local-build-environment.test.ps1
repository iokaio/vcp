# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
$tokens=$null; $errors=$null
$builder=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../build-local-setup.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw $errors[0] }
$definition=$builder.Find({param($ast) $ast -is [Management.Automation.Language.FunctionDefinitionAst] -and $ast.Name -ceq 'Invoke-LocalProductionBuild'},$true)
. ([scriptblock]::Create($definition.Extent.Text))
$production=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../build-production.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw $errors[0] }
$guard=$production.Find({param($ast) $ast -is [Management.Automation.Language.ForEachStatementAst] -and $ast.Extent.Text -match 'Unexpected build override:'},$true)
if (-not $guard) { throw 'Production override guard missing' }
$names=@($guard.Condition.SafeGetValue())
$saved=@{}
foreach ($name in $names) { $saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process') }
$temporaryBase=[IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root=Join-Path $temporaryBase ('vcp-local-environment-' + [guid]::NewGuid().ToString('N'))
$recipe=@('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31
function Check([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }
function Reject([scriptblock]$Action,[string]$Expected) {
    $reason=$null
    try { & $Action | Out-Null } catch { $reason=$_.Exception.Message }
    Check ($reason -ceq $Expected) "Expected $Expected; actual: $reason"
}
try {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name,[NullString]::Value,'Process') }
    New-Item -ItemType Directory -Path $root | Out-Null
    $success=Join-Path $root 'success.ps1'; $failure=Join-Path $root 'failure.ps1'
    # Use the actual production override guard while omitting toolchain setup and
    # compilation. Direct production still rejects even canonical inherited flags.
    $prefix="param([string]`$OutputRoot,[int]`$Jobs)`n" + $guard.Extent.Text + "`n" + @'
if ($Jobs -ne 23 -or -not (Test-Path -LiteralPath $OutputRoot -PathType Container)) { throw 'Production parameters changed' }
'@
    [IO.File]::WriteAllText($success,$prefix + "`nWrite-Output 'fixture-build-receipt'")
    [IO.File]::WriteAllText($failure,$prefix + "`nthrow 'fixture-compiler-failure'")
    Check ((Invoke-LocalProductionBuild $success $root 23) -ceq 'fixture-build-receipt') 'Clean local environment failed'
    Check ($null -eq [Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')) 'Absent flags became defined'
    [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$recipe,'Process')
    Reject { & $success -OutputRoot $root -Jobs 23 } 'Unexpected build override: CARGO_ENCODED_RUSTFLAGS'
    for ($attempt=0; $attempt -lt 2; $attempt++) {
        Check ((Invoke-LocalProductionBuild $success $root 23) -ceq 'fixture-build-receipt') 'Retained recipe flags prevented retry'
        Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq $recipe) 'Retained flags were not restored on success'
    }
    Reject { Invoke-LocalProductionBuild $failure $root 23 } 'fixture-compiler-failure'
    Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq $recipe) 'Retained flags were not restored on failure'
    $customFlags=@(
        'unqualified-custom-flags',
        ($recipe + [char]31 + '-C' + [char]31 + 'opt-level=0'),
        $recipe.Replace('-C','-c'),
        (@('-C','target-feature=+crt-static','-C','link-arg=/STACK:8388608') -join [char]31),
        (@('-C','target-feature=+crt-static') -join [char]31)
    )
    foreach ($custom in $customFlags) {
        [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$custom,'Process')
        Reject { Invoke-LocalProductionBuild $success $root 23 } 'Unexpected build override: CARGO_ENCODED_RUSTFLAGS'
        Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq $custom) 'Custom caller flags changed'
    }
    [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$recipe,'Process')
    [Environment]::SetEnvironmentVariable('RUSTFLAGS','unqualified-custom-flags','Process')
    Reject { Invoke-LocalProductionBuild $success $root 23 } 'Unexpected build override: RUSTFLAGS'
    Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq $recipe) 'Retained flags were lost on another override failure'
    Check ([Environment]::GetEnvironmentVariable('RUSTFLAGS','Process') -ceq 'unqualified-custom-flags') 'Another caller override changed'
    Write-Output 'PASS local build retained flags, repeated retry, success/failure restoration and custom override rejection'
} finally {
    foreach ($name in $names) {
        $value=if ($null -eq $saved[$name]) { [NullString]::Value } else { $saved[$name] }
        [Environment]::SetEnvironmentVariable($name,$value,'Process')
    }
    $resolved=[IO.Path]::GetFullPath($root)
    if ($resolved.StartsWith($temporaryBase,[StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-local-environment-*') {
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
