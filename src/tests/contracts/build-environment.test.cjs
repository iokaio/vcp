// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const {spawnSync} = require('node:child_process');
const source = path.resolve(__dirname, '../../../scripts/build-production.ps1');

for (const mode of ['success', 'failure', 'early-failure', 'custom-override', 'canonical-override']) {
  test(`production environment is restored after ${mode}`, {skip: process.platform !== 'win32'}, t => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-build-environment-'));
    t.after(() => fs.rmSync(root, {recursive: true, force: true}));
    const runner = path.join(root, 'run.ps1');
    fs.writeFileSync(runner, `param([string]$Source,[string]$Mode)
$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Production wrapper parse failed'}
$scope=@($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })
if($scope.Count -ne 1 -or -not $scope[0].Finally){throw 'Production environment finally scope required'}
# Execute the actual snapshot, override guards, flag setup and finally cleanup.
# Omit source/tool/compilation operations: this checks the environment boundary
# without creating product artifacts or depending on a local native toolchain.
$snapshot=@($ast.EndBlock.Statements | Where-Object { $_.Extent.EndOffset -lt $scope[0].Extent.StartOffset -and $_.Extent.Text -match 'callerEnvironment' })
if($snapshot.Count -ne 2){throw 'Caller snapshot required before recipe execution'}
$statements=@($scope[0].Body.Statements | Where-Object {
    $_.Extent.Text -match '^\\$env:(RUST_MIN_STACK|CARGO_ENCODED_RUSTFLAGS)\\s*=' -or
    $_.Extent.Text -match '^\\$rustflags\\s*=' -or
    $_.Extent.Text -match 'Unexpected build override:|Release profile overrides are not allowed'
})
if($statements.Count -ne 5){throw 'Actual production environment setup/guards required'}
$body=($statements | ForEach-Object { $_.Extent.Text }) -join "\n"
$traps=($scope[0].Body.Traps | ForEach-Object { $_.Extent.Text }) -join "\n"
$recipe=[scriptblock]::Create((($snapshot | ForEach-Object { $_.Extent.Text }) -join "\n")+"\ntry {\n"+$traps+"\n"+@'
$env:PATH='inert developer shell path'
$env:VCP_BUILD_ENV_TEST_EXISTING='changed by developer shell'
$env:VCP_BUILD_ENV_TEST_NEW='added by developer shell'
[Environment]::SetEnvironmentVariable('VCP_BUILD_ENV_TEST_REMOVED',[NullString]::Value,'Process')
if($fixtureMode -eq 'early-failure'){throw 'early-source-sentinel'}
'@+"\n"+$body+"\n"+@'
if($env:CARGO_ENCODED_RUSTFLAGS -cne (@('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31)){throw 'Canonical compiler flags missing'}
if($env:RUST_MIN_STACK -cne '16777216'){throw 'Compiler stack setting missing'}
$script:reachedCompiler++
if($fixtureMode -eq 'failure'){throw 'compiler-failure-sentinel'}
Write-Output 'inert-build-receipt'
'@+"\n} finally "+$scope[0].Finally.Extent.Text)
function Check([bool]$Condition,[string]$Message){if(-not $Condition){throw $Message}}
function Environment-Snapshot {
    $values=[ordered]@{}
    Get-ChildItem Env: | Sort-Object Name | ForEach-Object { $values[$_.Name]=$_.Value }
    return ($values | ConvertTo-Json -Compress)
}
# Fixture isolation is confined to this child PowerShell process.
$guard=$statements | Where-Object { $_ -is [Management.Automation.Language.ForEachStatementAst] }
foreach($name in $guard.Condition.SafeGetValue()){[Environment]::SetEnvironmentVariable($name,[NullString]::Value,'Process')}
Get-ChildItem Env: | Where-Object Name -like 'CARGO_PROFILE_RELEASE_*' | ForEach-Object { [Environment]::SetEnvironmentVariable($_.Name,[NullString]::Value,'Process') }
$env:RUST_MIN_STACK='caller-stack'
$env:VCP_BUILD_ENV_TEST_EXISTING='caller-existing'
$env:VCP_BUILD_ENV_TEST_REMOVED='caller-removed'
[Environment]::SetEnvironmentVariable('VCP_BUILD_ENV_TEST_NEW',[NullString]::Value,'Process')
Check (-not (Test-Path Env:VCP_BUILD_ENV_TEST_NEW)) 'New-variable fixture must start absent'
if($Mode -eq 'custom-override'){$env:CARGO_ENCODED_RUSTFLAGS='unqualified-custom-override'}
if($Mode -eq 'canonical-override'){$env:CARGO_ENCODED_RUSTFLAGS=@('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31}
$before=Environment-Snapshot
$progressPath='inert-progress';$buildPhase='cargo';$buildProcess=$null
function Write-VcpBuildPhase { $null=@(1 | ForEach-Object { $_ }) }
$script:reachedCompiler=0;$fixtureMode=$Mode;$failure=$null;$output=$null
try {$output=& $recipe} catch {$failure=$_.Exception.Message}
Check ((Environment-Snapshot) -ceq $before) 'Caller environment changed after recipe exit'
switch($Mode){
    success {Check (-not $failure -and $output -ceq 'inert-build-receipt') 'Success output lost'}
    failure {Check ($failure -ceq 'compiler-failure-sentinel') 'Compiler failure replaced'}
    early-failure {Check ($failure -ceq 'early-source-sentinel') 'Early failure replaced'}
    default {Check ($failure -ceq 'Unexpected build override: CARGO_ENCODED_RUSTFLAGS') 'Inherited override accepted or replaced';Check ($script:reachedCompiler -eq 0) 'Override reached compiler'}
}
if($Mode -in @('success','failure','early-failure')){
    $fixtureMode='success';$output=& $recipe
    Check ($output -ceq 'inert-build-receipt') 'Same-session retry failed'
    Check ((Environment-Snapshot) -ceq $before) 'Retry changed caller environment'
    $expected=if($Mode -eq 'early-failure'){1}else{2}
    Check ($script:reachedCompiler -eq $expected) 'Compiler/retry did not execute'
}
Write-Output ('PASS '+$Mode)
`);
    const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Source', source, '-Mode', mode],
      {encoding: 'utf8', windowsHide: true, timeout: 10000});
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr + result.stdout);
    assert.match(result.stdout, new RegExp(`PASS ${mode}`));
  });
}
