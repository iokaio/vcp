# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('native','setup')][string]$Stage,
    [Parameter(Mandatory)][string]$BuildReceipt,
    [Parameter(Mandatory)][string[]]$Rows,
    [Parameter(Mandatory)][string]$ToolsManifest,
    [Parameter(Mandatory)][string]$OutputFile
)
$ErrorActionPreference = 'Stop'
$build = Get-Content -LiteralPath $BuildReceipt -Raw | ConvertFrom-Json -Depth 100
$tools = Get-Content -LiteralPath $ToolsManifest -Raw | ConvertFrom-Json -Depth 30
$record = [ordered]@{
    schema='vcp-authenticode-transform/1'; stage=$Stage; status='verified'
    candidate_id=$build.release.candidate_id; reviewed_commit=$build.release.reviewed_commit
    build_receipt_sha256=(Get-FileHash -LiteralPath $BuildReceipt -Algorithm SHA256).Hash.ToLowerInvariant()
    policy=$build.release.signing
    tools=@{signtool_sha256=$tools.signtool.sha256;dlib_sha256=$tools.dlib.sha256}
    files=@($Rows | ForEach-Object { Get-Content -LiteralPath $_ -Raw | ConvertFrom-Json -Depth 30 })
}
# A new receipt never overwrites a prior signing attempt or compiler record.
$stream = [IO.FileStream]::new([IO.Path]::GetFullPath($OutputFile),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
try {
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($record | ConvertTo-Json -Depth 40)+"`n")
    $stream.Write($bytes,0,$bytes.Length)
} finally { $stream.Dispose() }
$node = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
& $node -e 'const p=require(process.argv[1]),s=require(process.argv[2]),r=p.json(process.argv[3]),b=p.json(process.argv[4]);s.validateReceipt(r,b.release.signing,{stage:r.stage,candidateId:b.release.candidate_id,reviewedCommit:b.release.reviewed_commit,buildReceiptSha256:p.fileHash(process.argv[4]),inputs:r.stage==="native"?{engine:b.executable_sha256,launcher:b.launcher_sha256}:undefined});' (Join-Path $PSScriptRoot 'provenance.cjs') (Join-Path $PSScriptRoot 'signing.cjs') $OutputFile $BuildReceipt
if ($LASTEXITCODE -ne 0) { throw 'Signing receipt validation failed; evidence retained' }
return [ordered]@{status='signed';receipt='signing-receipt.json';receipt_sha256=(Get-FileHash -LiteralPath $OutputFile -Algorithm SHA256).Hash.ToLowerInvariant();transformation=$record}
