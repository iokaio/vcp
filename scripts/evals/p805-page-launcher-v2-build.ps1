# SPDX-License-Identifier: Apache-2.0
# Offline, separately identified launcher proposal. Never changes a frozen cohort.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$NodePath,[Parameter(Mandatory)][string]$CompilerPath)
$ErrorActionPreference='Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$source=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'p805-page-check-launcher-v2.rs')).Path
$node=(Resolve-Path -LiteralPath $NodePath).Path
$compiler=(Resolve-Path -LiteralPath $CompilerPath).Path
function Digest([string]$Path) { (Get-FileHash -LiteralPath $Path).Hash.ToLowerInvariant() }
$before=@{source=(Digest $source);node=(Digest $node);compiler=(Digest $compiler);builder=(Digest $PSCommandPath)}
if($before.node -cne '8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1'){throw 'Pinned Node 26.9.0 required'}
if($before.compiler -cne 'e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a'){throw 'Pinned Rust 1.95.0 compiler required'}
$root=Join-Path $repository ('artifacts/p805-page-launcher-v2-'+[guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $root | Out-Null
$launcher=Join-Path $root 'page-check-v2.exe';$tests=Join-Path $root 'launcher-tests.exe'
$arguments=@('--edition=2021','--crate-name','vcp_p805_page_launcher_v2',$source,'-o',$launcher)
$testArguments=@('--edition=2021','--crate-name','vcp_p805_page_launcher_v2','--test',$source,'-o',$tests)
$receipt=[ordered]@{schema='p805-page-launcher-build/2';disposition='future-binding-proposal';paid_authorization=$false;model_calls=0;started_at=[DateTime]::UtcNow.ToString('o');source=$source;source_sha256=$before.source;node=$node;node_sha256=$before.node;compiler=$compiler;compiler_sha256=$before.compiler;arguments=$arguments;test_arguments=$testArguments;embedded=@{VCP_P805_NODE=$node;VCP_P805_SYSTEMROOT=$env:SystemRoot};launcher=$launcher;exit_code=1;inputs_unchanged=$false;builder=$PSCommandPath;builder_sha256=$before.builder}
$oldNode=$env:VCP_P805_NODE;$oldSystemRoot=$env:VCP_P805_SYSTEMROOT
try {
    $env:VCP_P805_NODE=$node;$env:VCP_P805_SYSTEMROOT=$env:SystemRoot
    $receipt.compiler_version=(& $compiler --version --verbose) -join "`n"
    if($LASTEXITCODE -ne 0){throw 'Compiler probe failed'}
    & $compiler @arguments *> (Join-Path $root 'build.log')
    if($LASTEXITCODE -ne 0){throw 'Launcher compilation failed'}
    & $compiler @testArguments *> (Join-Path $root 'test-build.log')
    if($LASTEXITCODE -ne 0){throw 'Parser test compilation failed'}
    & $tests *> (Join-Path $root 'parser-tests.log')
    $receipt.parser_test_exit_code=$LASTEXITCODE
    if($LASTEXITCODE -ne 0){throw 'Parser regression failed'}
    $receipt.inputs_unchanged=($before.source -ceq (Digest $source) -and $before.node -ceq (Digest $node) -and $before.compiler -ceq (Digest $compiler) -and $before.builder -ceq (Digest $PSCommandPath))
    if(-not $receipt.inputs_unchanged){throw 'Build inputs changed'}
    $receipt.launcher_sha256=Digest $launcher
    $receipt.exit_code=0
} catch { $receipt.failure=$_.Exception.Message }
finally {
    $env:VCP_P805_NODE=$oldNode;$env:VCP_P805_SYSTEMROOT=$oldSystemRoot
    $receipt.completed_at=[DateTime]::UtcNow.ToString('o')
    $receipt.logs=@{}
    foreach($name in @('build.log','test-build.log','parser-tests.log')) { if(Test-Path -LiteralPath (Join-Path $root $name)){$receipt.logs[$name]=Digest (Join-Path $root $name)} }
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $root 'build-receipt.json') -Encoding utf8NoBOM
}
Write-Output (Join-Path $root 'build-receipt.json')
if($receipt.exit_code -ne 0){exit 1}
