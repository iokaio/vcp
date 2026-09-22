# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Acquire','Verify')][string]$Action,
    [Parameter(Mandatory)][string]$Destination,
    [string]$Specification
)
$ErrorActionPreference = 'Stop'
if (-not $Specification) { $Specification = Join-Path $PSScriptRoot '../models/minilm-assets.json' }
$spec = Get-Content -LiteralPath $Specification -Raw | ConvertFrom-Json
if ($spec.schema_version -ne 1 -or $spec.repository -cne 'https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2' -or $spec.revision -cnotmatch '^[a-f0-9]{40}$' -or $spec.license_declaration -cne 'Apache-2.0' -or $spec.files.Count -ne 10) { throw 'Unsupported pinned model specification' }
$root = [IO.Path]::GetFullPath($Destination)
$packageRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\','/')
if ($root -ieq $packageRoot -or $root.StartsWith($packageRoot+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Provision models outside the installation and source checkout' }
$seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($file in $spec.files) {
    if (-not $file.path -or $file.path.Contains('\') -or @($file.path.Split('/') | Where-Object { -not $_ -or $_ -in @('.','..') -or $_ -match '[<>:"|?*\x00-\x1f]' -or $_ -match '[. ]$' }).Count -or -not $seen.Add($file.path) -or $file.bytes -le 0 -or $file.bytes -gt 104857600 -or $file.sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid bounded model asset identity' }
}
function Assert-PlainAncestors([string]$Selected) {
    for ($item = [IO.Path]::GetFullPath($Selected); $item; $item = [IO.Directory]::GetParent($item)?.FullName) {
        if ((Test-Path -LiteralPath $item) -and ((Get-Item -LiteralPath $item -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Redirected model path refused' }
    }
}
Assert-PlainAncestors $root
if ($Action -eq 'Acquire') {
    if (Test-Path -LiteralPath $root) { throw 'Acquisition requires a new directory; verify existing assets separately' }
    New-Item -ItemType Directory -Path $root | Out-Null
    $claim = @{schema='vcp-model-acquisition/1'; revision=$spec.revision; specification_sha256=(Get-FileHash -LiteralPath $Specification).Hash.ToLowerInvariant(); status='acquiring'}
    $claim | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root '.vcp-acquisition.json') -Encoding utf8
    $client = [Net.Http.HttpClient]::new(); $client.Timeout = [TimeSpan]::FromMinutes(5)
    try {
        foreach ($file in $spec.files) {
            $target = Join-Path $root $file.path; $partial = "$target.partial"
            New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
            Assert-PlainAncestors $target
            $deadline = [Threading.CancellationTokenSource]::new([TimeSpan]::FromMinutes(5))
            $response = $client.GetAsync("$($spec.repository)/resolve/$($spec.revision)/$($file.path)",[Net.Http.HttpCompletionOption]::ResponseHeadersRead,$deadline.Token).GetAwaiter().GetResult()
            $response.EnsureSuccessStatusCode() | Out-Null
            $inputStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $outputStream = [IO.File]::Open($partial,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
            [int64]$total = 0; $buffer = [byte[]]::new(65536)
            try {
                while (($count = $inputStream.ReadAsync($buffer,0,$buffer.Length,$deadline.Token).GetAwaiter().GetResult()) -gt 0) {
                    $total += $count
                    if ($total -gt $file.bytes) { throw 'Model response exceeded pinned byte count' }
                    $outputStream.Write($buffer,0,$count)
                }
                $outputStream.Flush($true)
            } finally { $outputStream.Dispose(); $inputStream.Dispose(); $response.Dispose(); $deadline.Dispose() }
            if ($total -ne $file.bytes -or (Get-FileHash -LiteralPath $partial).Hash.ToLowerInvariant() -cne $file.sha256) { throw 'Downloaded model bytes failed verification; partial evidence retained' }
            Move-Item -LiteralPath $partial -Destination $target
        }
    } catch {
        $claim.status = 'failed'; $claim.reason = $_.Exception.Message
        $claim | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root '.vcp-acquisition.json') -Encoding utf8
        throw
    } finally { $client.Dispose() }
}
[int64]$bytes = 0
foreach ($file in $spec.files) {
    $target = Join-Path $root $file.path; Assert-PlainAncestors $target
    $item = Get-Item -LiteralPath $target -Force
    if ($item.PSIsContainer -or $item.Length -ne $file.bytes -or (Get-FileHash -LiteralPath $target).Hash.ToLowerInvariant() -cne $file.sha256) { throw "Model verification failed: $($file.path)" }
    $bytes += $item.Length
}
if ($Action -eq 'Acquire') { $claim.status = 'verified'; $claim | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root '.vcp-acquisition.json') -Encoding utf8 }
@{schema='vcp-model-verification/1';status='passed';revision=$spec.revision;files=$spec.files.Count;bytes=$bytes;license=$spec.license_declaration;license_source=$spec.license_source}|ConvertTo-Json -Compress
