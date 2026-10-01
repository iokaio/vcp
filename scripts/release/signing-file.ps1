# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$File,
    [Parameter(Mandatory)][ValidateSet('engine','launcher','setup','uninstaller')][string]$Role,
    [Parameter(Mandatory)][string]$OutputRoot,
    [string]$ToolsManifest = $env:VCP_SIGNING_TOOLS_MANIFEST
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows Authenticode verification required' }
if ([string]::IsNullOrWhiteSpace($ToolsManifest)) { throw 'Pinned signing tools manifest required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$policy = (Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json).signing
if ($policy.status -cne 'signed') { throw 'Signed release policy required' }
$out = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $out) { throw 'Fresh signing evidence directory required' }
$inputFile = Get-Item -LiteralPath ([IO.Path]::GetFullPath($File)) -Force
if ($inputFile.PSIsContainer) { throw 'Signing requires an ordinary file' }
for ($current=$inputFile; $current; $current=if ($current -is [IO.DirectoryInfo]) {$current.Parent} else {$current.Directory}) {
    if ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Signing refuses redirected files and ancestors' }
}
$tools = Get-Content -LiteralPath $ToolsManifest -Raw | ConvertFrom-Json
$toolRoot = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ToolsManifest))
if ($tools.signtool.path -ine (Join-Path $toolRoot 'sdk/bin/10.0.26100.0/x64/signtool.exe') -or
    $tools.dlib.path -ine (Join-Path $toolRoot 'client/bin/x64/Azure.CodeSigning.Dlib.dll') -or
    $tools.metadata -ine (Join-Path $toolRoot 'metadata.json')) { throw 'Signing tools must remain in their provisioned directory' }
if ($tools.schema -cne 'vcp-signing-tools/1' -or
    $tools.signtool.sha256 -cne '0e2896a643540cad3b7078c796c2135911d224f3e76a8284b71673d621220921' -or
    $tools.dlib.sha256 -cne 'a359b420f676bc0223a379a84ca8369588ae7f265fd4f3e761e3425cba376916') { throw 'Unexpected signing tools' }
# Check the whole extracted dependency closure against the immutable packages,
# including managed assemblies and runtime configuration loaded by the Dlib.
foreach ($package in @(
    @{name='client';sha256='3bfcf1e0a3cb42af1692f0a8ed45c15de070c2de86f28a59b2795d904d8a920f'},
    @{name='sdk';sha256='180deb372659029864c10a0c04787833234d64aacd1d2c0661d2c00295d8e022'}
)) {
    $archive = Join-Path $toolRoot ($package.name+'.nupkg')
    if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $package.sha256) { throw 'Signing tool archive changed' }
    $extractedRoot = Join-Path $toolRoot $package.name
    $extractedFiles = @(Get-ChildItem -LiteralPath $extractedRoot -Recurse -Force)
    foreach ($item in @((Get-Item -LiteralPath $archive),(Get-Item -LiteralPath $extractedRoot))+$extractedFiles) {
        for ($current=$item; $current; $current=if ($current -is [IO.DirectoryInfo]) {$current.Parent} else {$current.Directory}) {
            if ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Signing tools refuse redirected files and ancestors' }
        }
    }
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $entries = @($zip.Entries | Where-Object { $_.Name -ne '' })
        if (@($extractedFiles | Where-Object { -not $_.PSIsContainer }).Count -ne $entries.Count) { throw 'Signing dependency closure changed' }
        foreach ($entry in $entries) {
            $stream = $entry.Open()
            try { $digest = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant() } finally { $stream.Dispose() }
            if ((Get-FileHash -LiteralPath (Join-Path $extractedRoot $entry.FullName)).Hash.ToLowerInvariant() -cne $digest) { throw 'Signing dependency changed' }
        }
    } finally { $zip.Dispose() }
}
foreach ($tool in @($tools.signtool,$tools.dlib,@{path=$tools.metadata;sha256=$tools.metadata_sha256})) {
    if ((Get-FileHash -LiteralPath $tool.path).Hash.ToLowerInvariant() -cne $tool.sha256) { throw 'Signing tools changed since provisioning' }
}
$metadata = Get-Content -LiteralPath $tools.metadata -Raw | ConvertFrom-Json
if ($metadata.Endpoint -cne 'https://wus2.codesigning.azure.net' -or $metadata.CodeSigningAccountName -cne 'ioka-llc-signing' -or $metadata.CertificateProfileName -cne 'WritingForgePro') { throw 'Unexpected signing account' }
$excluded = @('EnvironmentCredential','WorkloadIdentityCredential','ManagedIdentityCredential','SharedTokenCacheCredential','VisualStudioCredential','VisualStudioCodeCredential','AzurePowerShellCredential','AzureDeveloperCliCredential','InteractiveBrowserCredential')
if (@($metadata.ExcludeCredentials).Count -ne $excluded.Count -or (Compare-Object $excluded @($metadata.ExcludeCredentials) -CaseSensitive)) { throw 'Signing requires Azure CLI credentials exclusively' }
if ((Get-AuthenticodeSignature -LiteralPath $inputFile.FullName).Status -ne 'NotSigned') { throw 'Signing requires an unsigned input' }
New-Item -ItemType Directory -Path $out -Force | Out-Null
$original = Join-Path $out 'unsigned.exe'
Copy-Item -LiteralPath $inputFile.FullName -Destination $original
$signLog = Join-Path $out 'sign.log'
& $tools.signtool.path sign /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 /dlib $tools.dlib.path /dmdf $tools.metadata $inputFile.FullName *> $signLog
if ($LASTEXITCODE -ne 0) { throw "Signing failed; retained log: $signLog" }
$verifyLog = Join-Path $out 'verify.log'
& $tools.signtool.path verify /pa /all /tw /v $inputFile.FullName *> $verifyLog
if ($LASTEXITCODE -ne 0) { throw "Signature verification failed; retained log: $verifyLog" }
$signature = Get-AuthenticodeSignature -LiteralPath $inputFile.FullName
if ($signature.Status -ne 'Valid' -or -not $signature.SignerCertificate -or -not $signature.TimeStamperCertificate) { throw 'Valid timestamped Authenticode signature required' }
if ($signature.SignerCertificate.Subject -cne $policy.publisher) { throw 'Unexpected signing publisher' }
$eku = @($signature.SignerCertificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' } | ForEach-Object { $_.EnhancedKeyUsages | ForEach-Object Value })
if ($policy.identity_eku -cnotin $eku -or '1.3.6.1.5.5.7.3.3' -cnotin $eku) { throw 'Required publisher identity and code signing EKUs are absent' }
$transformJson = & node -e 'const s=require(process.argv[1]);console.log(JSON.stringify(s.verifyPeTransformation(process.argv[2],process.argv[3])))' (Join-Path $PSScriptRoot 'signing.cjs') $original $inputFile.FullName
if ($LASTEXITCODE -ne 0) { throw 'Signed executable changed beyond permitted Authenticode transformation' }
$row = $transformJson | ConvertFrom-Json -AsHashtable
$row.role=$Role
$row.signature=@{
    status='Valid';subject=$signature.SignerCertificate.Subject
    certificate_sha256=$signature.SignerCertificate.GetCertHashString([Security.Cryptography.HashAlgorithmName]::SHA256).ToLowerInvariant()
    identity_eku=$policy.identity_eku;timestamp_present=$true
    timestamp_certificate_sha256=$signature.TimeStamperCertificate.GetCertHashString([Security.Cryptography.HashAlgorithmName]::SHA256).ToLowerInvariant()
}
$row.verification=@{exit_code=0;log_sha256=(Get-FileHash -LiteralPath $verifyLog).Hash.ToLowerInvariant()}
$rowPath=Join-Path $out 'row.json'
$row | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $rowPath -Encoding utf8NoBOM
Write-Output $rowPath
