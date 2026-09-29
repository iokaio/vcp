# SPDX-License-Identifier: Apache-2.0
# Prepare the exact, one-shot CS-3 DeepSeek endpoint conformance input.
# This command captures public metadata only. It does not read a credential or
# dispatch a paid request; vcp-provider-conformance owns that separate step.
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PrivateDirectory,
    [ValidateSet('gmicloud/fp8','deepinfra/fp4','friendli')][string]$Endpoint = 'gmicloud/fp8'
)
$ErrorActionPreference = 'Stop'
$model = 'deepseek/deepseek-v3.2'
$directory = [IO.Path]::GetFullPath($PrivateDirectory)
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$separator = [IO.Path]::DirectorySeparatorChar
if ($directory.Equals($repository, [StringComparison]::OrdinalIgnoreCase) -or
    $directory.StartsWith($repository + $separator, [StringComparison]::OrdinalIgnoreCase) -or
    $repository.StartsWith($directory + $separator, [StringComparison]::OrdinalIgnoreCase) -or
    (Test-Path -LiteralPath $directory)) {
    throw 'A new private directory outside the repository is required'
}
New-Item -ItemType Directory -Path $directory | Out-Null
$catalog = Join-Path $directory 'deepseek-v3.2-endpoints.json'
$response = Invoke-WebRequest -Uri "https://openrouter.ai/api/v1/models/$model/endpoints" -Method Get -TimeoutSec 60 -SkipHttpErrorCheck
if ($response.StatusCode -ne 200) { throw "OpenRouter endpoint catalog returned HTTP $($response.StatusCode)" }
[IO.File]::WriteAllText($catalog, $response.Content, [Text.UTF8Encoding]::new($false))
$parsed = $response.Content | ConvertFrom-Json
$matches = @($parsed.data.endpoints | Where-Object tag -CEQ $Endpoint)
if ($parsed.data.id -cne $model -or $matches.Count -ne 1) { throw 'Exact DeepSeek endpoint is absent or ambiguous' }
$selected = $matches[0]
foreach ($parameter in @('tools','tool_choice','max_tokens')) {
    if (@($selected.supported_parameters) -cnotcontains $parameter) { throw "Endpoint lacks required parameter: $parameter" }
}
if ($selected.status -ne 0 -or $selected.context_length -lt 163840 -or $selected.max_completion_tokens -lt 2048 -or
    [string]::IsNullOrWhiteSpace("$($selected.pricing.prompt)") -or [string]::IsNullOrWhiteSpace("$($selected.pricing.completion)")) {
    throw 'Endpoint availability, bounds or prices are unsuitable'
}
$observed = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$validUntil = $observed + [int64][TimeSpan]::FromHours(24).TotalMilliseconds
$spec = [ordered]@{
    catalog = $catalog
    catalog_sha256 = (Get-FileHash -LiteralPath $catalog -Algorithm SHA256).Hash.ToLowerInvariant()
    model = $model
    endpoint = $Endpoint
    request_price_limit = '0.001'
    cap_usd = '0.250000'
    max_output_tokens = 2048
    observed_at = "$observed"
    valid_until = "$validUntil"
}
$specFile = Join-Path $directory 'probe-spec.json'
[IO.File]::WriteAllText($specFile, ($spec | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
[ordered]@{
    schema = 'cs3-deepseek-qualification-preparation/1'
    model = $model
    endpoint = $Endpoint
    provider_name = $selected.provider_name
    quantization = $selected.quantization
    context_length = $selected.context_length
    max_completion_tokens = $selected.max_completion_tokens
    pricing = $selected.pricing
    supported_parameters = @($selected.supported_parameters)
    observed_at = "$observed"
    valid_until = "$validUntil"
    probe_cap_usd = '0.250000'
    probe_max_requests = 2
    spec = $specFile
    spec_sha256 = (Get-FileHash -LiteralPath $specFile -Algorithm SHA256).Hash.ToLowerInvariant()
    model_calls = 0
} | ConvertTo-Json -Depth 8 -Compress
