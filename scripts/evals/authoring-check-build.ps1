# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([switch]$Followup)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows checker qualification required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$directory = Join-Path $repository ('artifacts/cs1-checker-build/' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
function Source-Inputs {
    $capture = @'
const p=require('./scripts/evals/authoring-prepare.cjs');
const scope=process.argv[1]==='followup'?require('./scripts/evals/authoring-followup.cjs').checkerBuildScope:p.checkerBuildScope;
console.log(JSON.stringify(p.identity(process.cwd(),scope).files.map(({path,sha256})=>({path,sha256}))));
'@
    $mode = if ($Followup) { 'followup' } else { 'original' }
    $value = & node -e $capture $mode
    if ($LASTEXITCODE -ne 0) { throw 'Build source identity capture failed' }
    return $value
}
Push-Location $repository
try {
    $before = Source-Inputs
    $arguments = @('build','--manifest-path','src/third_party/codex/codex-rs/Cargo.toml','--locked','--offline','--target-dir','artifacts/codex-target','-j2','-p','vcp-cli','--features','qualification','--bin','vcp-authoring-check')
    $rustc = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Rust compiler unavailable' }
    & cargo @arguments *> (Join-Path $directory 'build.log')
    $code = $LASTEXITCODE
    $after = Source-Inputs
    $builtExecutable = Join-Path $repository 'artifacts/codex-target/debug/vcp-authoring-check.exe'
    $executable = Join-Path $directory 'vcp-authoring-check.exe'
    if ($code -eq 0) { Copy-Item -LiteralPath $builtExecutable -Destination $executable -ErrorAction Stop }
    $source = 'src/crates/vcp-cli/src/bin/vcp-authoring-check.rs'
    $fixture = 'src/evals/skills/authoring/manifest.json'
    $receipt = [ordered]@{
        schema = 'cs1-authoring-check-build/1'
        source = $source
        source_sha256 = (Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant()
        fixture_manifest = $fixture
        fixture_manifest_sha256 = (Get-FileHash -LiteralPath $fixture).Hash.ToLowerInvariant()
        executable = $executable
        executable_sha256 = if ($code -eq 0) { (Get-FileHash -LiteralPath $executable).Hash.ToLowerInvariant() } else { $null }
        cargo_command = @('cargo') + $arguments
        exit_code = $code
        toolchain = @{rustc=$rustc}
        source_inputs = @($before | ConvertFrom-Json)
        source_inputs_unchanged = $before -ceq $after
        builder = $PSCommandPath
        builder_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
        at = [DateTime]::UtcNow.ToString('o')
    }
    if ($Followup) {
        $receipt.schema = 'cs1-authoring-check-build/2'
        $receipt.fixture_manifests = @('src/evals/skills/authoring/manifest.json', 'src/evals/skills/authoring-followup/manifest.json') | ForEach-Object { @{path=$_; sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()} }
    }
    $file = Join-Path $directory 'build-receipt.json'
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $file -Encoding utf8NoBOM
    if ($code -ne 0 -or -not $receipt.source_inputs_unchanged) { throw "Checker build failed or source changed; see $file" }
    Write-Output $file
} finally { Pop-Location }
