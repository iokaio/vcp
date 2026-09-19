# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$OutputRoot,
    [ValidateRange(0.01,10.00)][decimal]$SpendCapUsd = 10.00,
    [ValidateRange(1,256)][int]$MaxOutputTokens = 64
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/openrouter-live' }
$output = [IO.Path]::GetFullPath($OutputRoot)
if ($output.StartsWith([IO.Path]::GetFullPath((Join-Path $repository 'src')), [StringComparison]::OrdinalIgnoreCase)) {
    Write-Output '{"status":"not_run","reason":"Live evidence must be outside src"}'
    exit 3
}
$key = $env:OPENROUTER_API_KEY
if ([string]::IsNullOrWhiteSpace($key)) {
    $key = [Environment]::GetEnvironmentVariable('OPENROUTER_API_KEY', 'User')
}
if ([string]::IsNullOrWhiteSpace($key)) {
    Write-Output '{"status":"not_run","reason":"OPENROUTER_API_KEY is not configured"}'
    exit 3
}
$directory = Join-Path $output ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$manifest = Join-Path $directory 'manifest.json'
$models = @('openai/gpt-5.6-luna', 'anthropic/claude-sonnet-5')
$prompt = 'Reply with exactly: VCP_SMOKE_OK'
$record = [ordered]@{
    schema_version = 1
    task_id = 'P2-02'
    status = 'prepared'
    started_at = [DateTime]::UtcNow.ToString('o')
    gateway = 'openrouter'
    endpoint = 'https://openrouter.ai/api/v1/responses'
    spend_cap_usd = $SpendCapUsd.ToString('0.00', [Globalization.CultureInfo]::InvariantCulture)
    max_output_tokens = $MaxOutputTokens
    retries = 0
    input_scope = 'public synthetic marker only'
    models = $models
    estimated_max_usd = '0'
    observed_cost_usd = '0'
    stages = @()
}
function Save-Record { $record | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Decimal([object]$Value) {
    if ($null -eq $Value -or [string]::IsNullOrWhiteSpace("$Value")) { throw 'Missing price or cost' }
    [decimal]::Parse("$Value", [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture)
}
function Safe-Name([string]$Model) { $Model.Replace('/', '--').Replace(':', '-') }
Save-Record
try {
    $headers = @{
        Authorization = "Bearer $key"
        'Content-Type' = 'application/json'
        'HTTP-Referer' = 'https://github.com/iokaio/vcp'
        'X-Title' = 'VCP live smoke qualification'
    }
    $catalogResponse = Invoke-WebRequest -Uri 'https://openrouter.ai/api/v1/models' -Headers $headers -Method Get -TimeoutSec 60 -SkipHttpErrorCheck
    if ($catalogResponse.StatusCode -ne 200) { throw "OpenRouter catalog returned HTTP $($catalogResponse.StatusCode)" }
    $catalog = $catalogResponse.Content | ConvertFrom-Json
    $selected = @($catalog.data | Where-Object { $models -contains $_.id })
    $selected | Select-Object id, name, context_length, pricing, supported_parameters |
        ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $directory 'catalog.json') -Encoding utf8
    if ($selected.Count -ne $models.Count) { throw 'One or more fixed smoke models are absent from the current catalog' }

    $promptBytes = [Text.Encoding]::UTF8.GetByteCount($prompt)
    [decimal]$estimatedTotal = 0
    $admissions = @{}
    foreach ($model in $models) {
        $metadata = $selected | Where-Object id -eq $model | Select-Object -First 1
        if (@($metadata.supported_parameters) -notcontains 'max_tokens') { throw "$model lacks the required output bound" }
        # UTF-8 bytes conservatively bound token count for this ASCII-only prompt.
        $estimate = (Decimal $metadata.pricing.prompt) * $promptBytes +
            (Decimal $metadata.pricing.completion) * $MaxOutputTokens
        $estimatedTotal += $estimate
        $admissions[$model] = $estimate
    }
    $record.estimated_max_usd = $estimatedTotal.ToString('0.################', [Globalization.CultureInfo]::InvariantCulture)
    if ($estimatedTotal -gt $SpendCapUsd) { throw 'Worst-case admitted requests exceed the live spend cap' }
    $record.status = 'running'
    Save-Record

    [decimal]$observed = 0
    foreach ($model in $models) {
        if (($observed + [decimal]$admissions[$model]) -gt $SpendCapUsd) { throw 'Remaining spend cap is insufficient' }
        $body = [ordered]@{
            model = $model
            input = @([ordered]@{ role = 'user'; content = $prompt })
            max_output_tokens = $MaxOutputTokens
            stream = $false
        } | ConvertTo-Json -Depth 8 -Compress
        $started = [DateTime]::UtcNow
        $response = Invoke-WebRequest -Uri $record.endpoint -Headers $headers -Method Post -Body $body -TimeoutSec 120 -SkipHttpErrorCheck
        $stage = [ordered]@{
            model = $model
            submitted = $true
            http_status = [int]$response.StatusCode
            started_at = $started.ToString('o')
            ended_at = [DateTime]::UtcNow.ToString('o')
        }
        $response.Content | Set-Content -LiteralPath (Join-Path $directory "$(Safe-Name $model)-response.json") -Encoding utf8
        if ($response.StatusCode -ne 200) {
            $stage.status = 'fail'
            $stage.reason = "OpenRouter returned HTTP $($response.StatusCode)"
            $record.stages += $stage
            Save-Record
            continue
        }
        $parsed = $response.Content | ConvertFrom-Json
        $cost = Decimal $parsed.usage.cost
        $observed += $cost
        $text = @($parsed.output | ForEach-Object { @($_.content) } | Where-Object type -eq 'output_text' | ForEach-Object text) -join ''
        $stage.response_id = $parsed.id
        $stage.served_model = $parsed.model
        $stage.provider_status = $parsed.status
        $stage.input_tokens = $parsed.usage.input_tokens
        $stage.output_tokens = $parsed.usage.output_tokens
        $stage.cost_usd = $cost.ToString('0.################', [Globalization.CultureInfo]::InvariantCulture)
        $stage.status = if ($parsed.status -eq 'completed' -and $text.Trim() -eq 'VCP_SMOKE_OK') { 'pass' } else { 'fail' }
        if ($stage.status -ne 'pass') { $stage.reason = 'response did not satisfy the exact synthetic marker contract' }
        $record.stages += $stage
        $record.observed_cost_usd = $observed.ToString('0.################', [Globalization.CultureInfo]::InvariantCulture)
        Save-Record
        if ($observed -gt $SpendCapUsd) { throw 'Observed OpenRouter cost exceeded the live spend cap' }
    }
    $record.status = if (@($record.stages).Count -eq $models.Count -and @($record.stages | Where-Object status -ne 'pass').Count -eq 0) { 'pass' } else { 'fail' }
    $record.exit_code = if ($record.status -eq 'pass') { 0 } else { 1 }
} catch {
    $record.status = 'fail'
    $record.exit_code = 1
    $record.reason = $_.Exception.Message
} finally {
    $record.ended_at = [DateTime]::UtcNow.ToString('o')
    Save-Record
}
[pscustomobject]$record | Select-Object status, exit_code, reason, observed_cost_usd, @{n='manifest';e={$manifest}} | ConvertTo-Json -Compress
exit $record.exit_code
