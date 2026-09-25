# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Builds the CS-2 in-run developer checker and records an identity receipt. The
# receipt pins the exact executable approved for campaign process authority.
[CmdletBinding()]
param([string]$TargetDir = 'artifacts/codex-target')
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows checker qualification required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$directory = Join-Path $repository ('artifacts/cs2-checker-build/' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$scope = @('src/crates', 'src/evals/skills/developer', 'src/third_party/codex/codex-rs/Cargo.toml',
    'src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/rust-toolchain.toml')
function Get-SourceInputs {
    $capture = "const p=require('./scripts/evals/authoring-prepare.cjs');console.log(JSON.stringify(p.identity(process.cwd(),JSON.parse(process.argv[1])).files.map(({path,sha256})=>({path,sha256}))));"
    $value = & node -e $capture ($scope | ConvertTo-Json -Compress)
    if ($LASTEXITCODE -ne 0) { throw 'Build source identity capture failed' }
    return $value
}
Push-Location $repository
try {
    $before = Get-SourceInputs
    $arguments = @('build', '--manifest-path', 'src/third_party/codex/codex-rs/Cargo.toml', '--locked', '--offline',
        '--target-dir', $TargetDir, '-j2', '-p', 'vcp-cli', '--features', 'qualification', '--bin', 'vcp-developer-check')
    $rustc = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Rust compiler unavailable' }
    & cargo @arguments *> (Join-Path $directory 'build.log')
    $code = $LASTEXITCODE
    $after = Get-SourceInputs
    $targetRoot = if ([IO.Path]::IsPathRooted($TargetDir)) { $TargetDir } else { Join-Path $repository $TargetDir }
    $built = Join-Path ([IO.Path]::GetFullPath($targetRoot)) 'debug/vcp-developer-check.exe'
    $executable = Join-Path $directory 'vcp-developer-check.exe'
    if ($code -eq 0) { Copy-Item -LiteralPath $built -Destination $executable -ErrorAction Stop }
    $source = 'src/crates/vcp-cli/src/bin/vcp-developer-check.rs'
    $fixture = 'src/evals/skills/developer/manifest.json'
    $receipt = [ordered]@{
        schema = 'cs2-developer-check-build/1'
        source = $source
        source_sha256 = (Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant()
        fixture_manifest = $fixture
        fixture_manifest_sha256 = (Get-FileHash -LiteralPath $fixture).Hash.ToLowerInvariant()
        executable = $executable
        executable_sha256 = if ($code -eq 0) { (Get-FileHash -LiteralPath $executable).Hash.ToLowerInvariant() } else { $null }
        cargo_command = @('cargo') + $arguments
        exit_code = $code
        toolchain = @{ rustc = $rustc }
        source_scope = $scope
        source_inputs = @($before | ConvertFrom-Json)
        source_inputs_unchanged = $before -ceq $after
        builder = $PSCommandPath
        builder_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
        at = [DateTime]::UtcNow.ToString('o')
    }
    $file = Join-Path $directory 'build-receipt.json'
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $file -Encoding utf8NoBOM
    if ($code -ne 0 -or -not $receipt.source_inputs_unchanged) { throw "Checker build failed or source changed; see $file" }
    Write-Output $file
} finally { Pop-Location }
