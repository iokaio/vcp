# SPDX-License-Identifier: Apache-2.0
# Prepare the exact, one-shot CS-3 DeepSeek endpoint conformance input.
# This command captures public metadata only. It does not read a credential or
# dispatch a paid request; vcp-provider-conformance owns that separate step.
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PrivateDirectory,
    [ValidateSet('gmicloud/fp8','deepinfra/fp4','friendli')][string]$Endpoint = 'gmicloud/fp8',
    [string]$FriendliQualificationSupplement,
    [string]$FriendliQualificationSupplementSha256
)
$ErrorActionPreference = 'Stop'
$model = 'deepseek/deepseek-v3.2'
$directory = [IO.Path]::GetFullPath($PrivateDirectory)
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$separator = [IO.Path]::DirectorySeparatorChar
$probeCap = '0.250000'
$originalEndpoint = $null
if ($FriendliQualificationSupplement -or $FriendliQualificationSupplementSha256) {
    if ($Endpoint -cne 'friendli' -or -not [IO.Path]::IsPathFullyQualified($FriendliQualificationSupplement) -or
        $FriendliQualificationSupplementSha256 -cnotmatch '^[a-f0-9]{64}$') {
        throw 'Exact fixed Friendli qualification supplement required'
    }
    $node = (Get-Command node -CommandType Application -ErrorAction Stop).Source
    if ((Get-FileHash -LiteralPath $node -Algorithm SHA256).Hash.ToLowerInvariant() -cne
        '3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5') {
        throw 'Pinned physical qualification Node runtime required'
    }
    $authorization = & $node (Join-Path $PSScriptRoot 'cs3-skill-qualification-supplement.cjs') authorize-probe `
        $FriendliQualificationSupplement $FriendliQualificationSupplementSha256
    if ($LASTEXITCODE -ne 0) { throw 'Friendli qualification supplement authorization failed' }
    $funded = $authorization | ConvertFrom-Json
    if ($funded.cap_micros -ne 500000 -or $funded.request_ceiling -ne 2 -or
        $funded.combined_cap_micros -ne 99413737 -or $funded.combined_request_ceiling -ne 2633 -or
        -not [IO.Path]::GetFullPath($funded.qualification_claim_path).Equals(
            (Join-Path $directory 'probe-output/claim.json'), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Supplement does not fund this exact new two-request probe'
    }
    if (-not [IO.Path]::IsPathFullyQualified($funded.original_catalog.path) -or
        $funded.original_catalog.sha256 -cnotmatch '^[a-f0-9]{64}$') {
        throw 'Authenticated original Friendli catalog required'
    }
    $originalBytes = [IO.File]::ReadAllBytes($funded.original_catalog.path)
    if ($originalBytes.Length -gt 4MB -or
        [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($originalBytes)).ToLowerInvariant() -cne $funded.original_catalog.sha256) {
        throw 'Original Friendli catalog changed'
    }
    $originalCatalog = [Text.Encoding]::UTF8.GetString($originalBytes) | ConvertFrom-Json
    $originalMatches = @($originalCatalog.data.endpoints | Where-Object tag -CEQ 'friendli')
    if ($originalCatalog.data.id -cne $model -or $originalMatches.Count -ne 1) { throw 'Original Friendli endpoint differs' }
    $originalEndpoint = $originalMatches[0]
    $probeCap = '0.500000'
}
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
if ($null -ne $originalEndpoint) {
    # The retained canonical reservation is 249832 micros. Require the exact
    # same capabilities and tariffs before consuming the new two-request claim;
    # no low-charge assumption or independent token-estimation heuristic.
    function AdmissionMetadata($Entry) {
        $prices = [ordered]@{}
        foreach ($property in ($Entry.pricing.PSObject.Properties | Sort-Object Name -CaseSensitive)) { $prices[$property.Name] = $property.Value }
        [ordered]@{ tag=$Entry.tag; provider_name=$Entry.provider_name; context_length=$Entry.context_length;
            max_prompt_tokens=$Entry.max_prompt_tokens; max_completion_tokens=$Entry.max_completion_tokens; pricing=$prices } | ConvertTo-Json -Depth 8 -Compress
    }
    if ((AdmissionMetadata $selected) -cne (AdmissionMetadata $originalEndpoint)) {
        throw 'Friendli capabilities or tariffs changed from the funded conservative reservation'
    }
}
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
    cap_usd = $probeCap
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
    probe_cap_usd = $probeCap
    probe_max_requests = 2
    spec = $specFile
    spec_sha256 = (Get-FileHash -LiteralPath $specFile -Algorithm SHA256).Hash.ToLowerInvariant()
    model_calls = 0
} | ConvertTo-Json -Depth 8 -Compress
