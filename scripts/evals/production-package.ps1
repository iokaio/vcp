# SPDX-License-Identifier: Apache-2.0
# Shared strict artifact reader. Dot-source; no executable is launched here.
function Read-ProductionPackage {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$PackageResult,
        [Parameter(Mandatory)][string]$ExtractionRoot,
        [string]$SelectedExecutable,
        [string]$NodeExecutable
    )
    function Plain-Path([string]$Path) {
        $full=[IO.Path]::GetFullPath($Path)
        for($cursor=$full;$cursor;$cursor=[IO.Path]::GetDirectoryName($cursor)) {
            if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Redirected qualification artifact path rejected' }
        }
        return $full
    }
    function File-Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
    $resultPath=Plain-Path $PackageResult
    $resultHash=File-Hash $resultPath
    if ((Get-Item -LiteralPath $resultPath).Length -gt 32MB) { throw 'Distribution result exceeds bound' }
    $result=Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json -Depth 100
    if ($result.schema -cne 'vcp-distribution-result/1' -or $result.status -cne 'release-candidate' -or $result.package -cnotmatch '^[a-zA-Z0-9._-]+\.zip$' -or $result.archive_sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Strict production distribution result required' }
    $archive=Plain-Path (Join-Path (Split-Path -Parent $resultPath) $result.package)
    if ((File-Hash $archive) -cne $result.archive_sha256) { throw 'Production archive digest mismatch' }
    $extracted=Plain-Path $ExtractionRoot
    if (Test-Path -LiteralPath $extracted) { throw 'Fresh package verification directory required' }
    $zip=[IO.Compression.ZipFile]::OpenRead($archive)
    try {
        if ($zip.Entries.Count -gt 4097) { throw 'Archive entry count exceeds bound' }
        $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        [long]$expanded=0
        foreach($entry in $zip.Entries) {
            $name=$entry.FullName
            if (-not $name -or $name.Length -gt 512 -or $name.Contains('\') -or
                @($name.Split('/') | Where-Object { -not $_ -or $_ -in @('.','..') -or $_ -match '[<>:"|?*\x00-\x1f]' -or $_ -match '[. ]$' -or $_ -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)' }).Count -or -not $seen.Add($name)) { throw 'Unsafe or duplicate archive entry' }
            $expanded+=$entry.Length
            if ($entry.Length -gt 1GB -or $expanded -gt 2GB) { throw 'Archive expanded size exceeds bound' }
        }
        $names=@($seen | Sort-Object)
        $expected=@($result.manifest.files.path + 'manifest.json' | Sort-Object)
        if (($names | ConvertTo-Json -Compress) -cne ($expected | ConvertTo-Json -Compress)) { throw 'Production archive inventory mismatch' }
    } finally { $zip.Dispose() }
    New-Item -ItemType Directory -Path $extracted | Out-Null
    $zip=[IO.Compression.ZipFile]::OpenRead($archive)
    try {
        foreach($entry in $zip.Entries) {
            $destination=Join-Path $extracted $entry.FullName
            New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
            $inputStream=$entry.Open()
            $outputStream=[IO.File]::Open($destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
            try {
                [long]$copied=0; $buffer=[byte[]]::new(65536)
                while(($count=$inputStream.Read($buffer,0,$buffer.Length)) -gt 0) {
                    $copied+=$count
                    if($copied -gt $entry.Length){throw 'ZIP content exceeded its declared length'}
                    $outputStream.Write($buffer,0,$count)
                }
                if($copied -ne $entry.Length){throw 'ZIP content length mismatch'}
            } finally {$outputStream.Dispose();$inputStream.Dispose()}
        }
    } finally {$zip.Dispose()}
    $nodePath=if($NodeExecutable){(Resolve-Path -LiteralPath $NodeExecutable).Path}else{(Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source}
    $verifier=Join-Path $PSScriptRoot 'production-package.cjs'
    $validation=@{reader_sha256=(File-Hash (Join-Path $PSScriptRoot 'production-package.ps1'));verifier_sha256=(File-Hash $verifier);node_sha256=(File-Hash $nodePath);provenance_sha256=(File-Hash (Join-Path $PSScriptRoot '../release/provenance.cjs'));inventory_sha256=(File-Hash (Join-Path $PSScriptRoot '../package-inventory.cjs'))}
    foreach($name in @('manifest.json','build-receipt.json')) { if((Get-Item -LiteralPath (Join-Path $extracted $name)).Length -gt 32MB){throw 'Package metadata exceeds bound'} }
    $identityJson=& $nodePath $verifier $resultPath $extracted
    if ($LASTEXITCODE -ne 0) { throw 'Production package build/source/payload binding failed' }
    $identity=$identityJson | ConvertFrom-Json -Depth 20
    $executable=Join-Path $extracted 'vcp.exe'
    $origin='verified extracted payload'
    if ($SelectedExecutable) {
        $executable=Plain-Path $SelectedExecutable
        if ([IO.Path]::GetFileName($executable) -cne 'vcp.exe' -or (File-Hash $executable) -cne $identity.executable_sha256) { throw 'Selected native executable differs from strict package' }
        & $nodePath $verifier $resultPath (Split-Path -Parent $executable) | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Selected executable payload differs from strict package' }
        $origin='explicit package-matching payload; installation ownership is a separate gate'
    }
    if ((File-Hash $archive) -cne $result.archive_sha256 -or (File-Hash $resultPath) -cne $resultHash) { throw 'Archive or receipt changed during validation' }
    return @{receipt=$resultPath;receipt_sha256=$resultHash;archive=$archive;archive_sha256=$result.archive_sha256;
        executable=$executable;executable_sha256=$identity.executable_sha256;build_receipt_sha256=$identity.build_receipt_sha256;
        installer=(Join-Path $extracted 'tools/package-install.ps1');manifest=$result.manifest;release=$identity.release;
        package_root=$extracted;execution_origin=$origin;validation=$validation}
}
