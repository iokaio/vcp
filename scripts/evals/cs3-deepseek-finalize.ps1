# SPDX-License-Identifier: Apache-2.0
# Capture read-only attribution records for an already-settled CS-3 probe and
# prepare the exact offline provider-qualification join. No model is dispatched.
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ProbeDirectory
)
$ErrorActionPreference = 'Stop'
function Hash([string]$File) { (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant() }
function Source([string]$File) { [ordered]@{ path = [IO.Path]::GetFullPath($File); sha256 = Hash $File } }
$directory = [IO.Path]::GetFullPath($ProbeDirectory)
$specFile = Join-Path $directory 'probe-spec.json'
$resultFile = Join-Path $directory 'probe-output/result.json'
if (-not (Test-Path -LiteralPath $specFile -PathType Leaf) -or -not (Test-Path -LiteralPath $resultFile -PathType Leaf)) { throw 'Completed probe inputs are absent' }
$spec = Get-Content -LiteralPath $specFile -Raw | ConvertFrom-Json
$result = Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
if ($spec.model -cne 'deepseek/deepseek-v3.2' -or @('gmicloud/fp8','deepinfra/fp4') -cnotcontains $spec.endpoint -or
    $result.status -cne 'observed' -or $result.responses_text_tools -ne $true -or
    $result.ledger.active -cne '0' -or $result.ledger.unresolved -cne '0' -or @($result.responses).Count -ne 2) {
    throw 'Exact settled DeepSeek conformance pair required'
}
$key = $env:OPENROUTER_API_KEY
if ([string]::IsNullOrWhiteSpace($key)) { $key = [Environment]::GetEnvironmentVariable('OPENROUTER_API_KEY', 'User') }
if ([string]::IsNullOrWhiteSpace($key)) { throw 'OPENROUTER_API_KEY is required' }
$headers = @{ Authorization = "Bearer $key"; 'HTTP-Referer' = 'https://github.com/iokaio/vcp'; 'X-Title' = 'VCP CS-3 DeepSeek qualification' }
$catalog = Join-Path $directory 'qualification-catalog.json'
$catalogResponse = Invoke-WebRequest -Uri 'https://openrouter.ai/api/v1/models/deepseek/deepseek-v3.2/endpoints' -Method Get -TimeoutSec 60 -SkipHttpErrorCheck
if ($catalogResponse.StatusCode -ne 200) { throw "OpenRouter endpoint catalog returned HTTP $($catalogResponse.StatusCode)" }
[IO.File]::WriteAllText($catalog, $catalogResponse.Content, [Text.UTF8Encoding]::new($false))
$generationSources = @()
for ($index = 0; $index -lt 2; $index++) {
    $id = [string]$result.responses[$index].response_id
    if ($id -notmatch '^[A-Za-z0-9_-]{8,200}$') { throw 'Probe response ID is malformed' }
    $response = Invoke-WebRequest -Uri ('https://openrouter.ai/api/v1/generation?id=' + [Uri]::EscapeDataString($id)) -Headers $headers -Method Get -TimeoutSec 60 -SkipHttpErrorCheck
    if ($response.StatusCode -ne 200) { throw "OpenRouter generation record returned HTTP $($response.StatusCode)" }
    $file = Join-Path $directory ("generation-$index.json")
    [IO.File]::WriteAllText($file, $response.Content, [Text.UTF8Encoding]::new($false))
    $generationSources += Source $file
}
$observed = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
if ($observed -ge [int64]$spec.valid_until) { throw 'Original probe qualification window expired' }
$sources = [ordered]@{
    probe_spec = Source $specFile
    report = Source $resultFile
    generations = $generationSources
    catalog = Source $catalog
    observed_at = "$observed"
    valid_until = [string]$spec.valid_until
}
$sourcesFile = Join-Path $directory 'qualification-sources.json'
[IO.File]::WriteAllText($sourcesFile, ($sources | ConvertTo-Json -Depth 6 -Compress), [Text.UTF8Encoding]::new($false))
[ordered]@{
    schema = 'cs3-deepseek-qualification-sources/1'
    sources = $sourcesFile
    sources_sha256 = Hash $sourcesFile
    catalog_sha256 = Hash $catalog
    observed_at = "$observed"
    valid_until = [string]$spec.valid_until
    model_calls = 0
} | ConvertTo-Json -Compress
