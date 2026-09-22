# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][string]$NodePath,
    [Parameter(Mandatory=$true)][string]$CompilerPath
)
$ErrorActionPreference = 'Stop'
$taskSource = (Resolve-Path (Join-Path $PSScriptRoot 'builtin-debug-launcher.rs')).Path
$taskNode = (Resolve-Path -LiteralPath $NodePath).Path
$taskCompiler = (Resolve-Path -LiteralPath $CompilerPath).Path
$taskRepository = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$taskBuildRoot = Join-Path $taskRepository ('artifacts/p7-cr06-launcher-' + [guid]::NewGuid().ToString('N'))
$taskLauncher = Join-Path $taskBuildRoot 'cr06-check.exe'
function Get-TaskDigest([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
$taskBefore = @{
    source = Get-TaskDigest $taskSource
    node = Get-TaskDigest $taskNode
    compiler = Get-TaskDigest $taskCompiler
}
if ($taskBefore.node -ne '8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1') {
    throw 'Exact qualified portable Node26.9.0 required'
}
New-Item -ItemType Directory -Path $taskBuildRoot | Out-Null
$taskArguments = @('--edition=2021','--crate-name','vcp_cr06_launcher',$taskSource,'-o',$taskLauncher)
$taskPreviousNode = $env:VCP_CR06_NODE
$taskPreviousSystemRoot = $env:VCP_CR06_SYSTEMROOT
try {
    $env:VCP_CR06_NODE = $taskNode
    $env:VCP_CR06_SYSTEMROOT = $env:SystemRoot
    $taskCompilerVersion = (& $taskCompiler --version --verbose) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'Compiler version probe failed' }
    $taskOutput = (& $taskCompiler @taskArguments 2>&1) -join "`n"
    $taskExit = $LASTEXITCODE
} finally {
    $env:VCP_CR06_NODE = $taskPreviousNode
    $env:VCP_CR06_SYSTEMROOT = $taskPreviousSystemRoot
}
if ($taskExit -ne 0) { throw ('Launcher compilation failed: ' + $taskOutput) }
if ($taskBefore.source -ne (Get-TaskDigest $taskSource) -or
    $taskBefore.node -ne (Get-TaskDigest $taskNode) -or
    $taskBefore.compiler -ne (Get-TaskDigest $taskCompiler)) {
    throw 'Launcher build input changed during compilation'
}
$taskReceipt = [ordered]@{
    schema = 'p7-cr06-launcher-build/1'
    at = [DateTime]::UtcNow.ToString('o')
    source = $taskSource
    source_sha256 = $taskBefore.source
    node = $taskNode
    node_sha256 = $taskBefore.node
    compiler = $taskCompiler
    compiler_sha256 = $taskBefore.compiler
    compiler_version = $taskCompilerVersion
    arguments = $taskArguments
    embedded = @{VCP_CR06_NODE=$taskNode; VCP_CR06_SYSTEMROOT=$env:SystemRoot}
    launcher = $taskLauncher
    launcher_sha256 = Get-TaskDigest $taskLauncher
    exit_code = $taskExit
    compiler_output = $taskOutput
    inputs_unchanged = $true
    builder = $PSCommandPath
    builder_sha256 = Get-TaskDigest $PSCommandPath
}
$taskReceiptPath = Join-Path $taskBuildRoot 'build-receipt.json'
$taskReceipt | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $taskReceiptPath -Encoding utf8NoBOM
[pscustomobject]@{receipt=$taskReceiptPath; launcher=$taskLauncher; sha256=$taskReceipt.launcher_sha256} | ConvertTo-Json
