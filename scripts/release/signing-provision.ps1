# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputRoot)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') { throw 'Signing requires Windows x64' }
$out = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $out) { throw 'Fresh signing tools directory required' }
$runtime = @(& dotnet --list-runtimes | Where-Object { $_ -match '^Microsoft.NETCore.App 8\.' })
if ($LASTEXITCODE -ne 0 -or $runtime.Count -eq 0) { throw 'Install the x64 .NET 8 runtime before provisioning signing tools' }
New-Item -ItemType Directory -Path $out -Force | Out-Null
$specs = @(
    @{name='client';id='microsoft.trusted.signing.client';version='1.0.95';sha256='3bfcf1e0a3cb42af1692f0a8ed45c15de070c2de86f28a59b2795d904d8a920f'},
    @{name='sdk';id='microsoft.windows.sdk.buildtools';version='10.0.26100.4188';sha256='180deb372659029864c10a0c04787833234d64aacd1d2c0661d2c00295d8e022'}
)
foreach ($spec in $specs) {
    $archive = Join-Path $out ($spec.name+'.nupkg')
    Invoke-WebRequest -Uri "https://api.nuget.org/v3-flatcontainer/$($spec.id)/$($spec.version)/$($spec.id).$($spec.version).nupkg" -OutFile $archive
    if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $spec.sha256) { throw 'Signing tool package hash mismatch' }
    Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $out $spec.name)
}
$metadata = Join-Path $out 'metadata.json'
@{
    Endpoint='https://wus2.codesigning.azure.net';CodeSigningAccountName='ioka-llc-signing';CertificateProfileName='WritingForgePro'
    ExcludeCredentials=@('EnvironmentCredential','WorkloadIdentityCredential','ManagedIdentityCredential','SharedTokenCacheCredential','VisualStudioCredential','VisualStudioCodeCredential','AzurePowerShellCredential','AzureDeveloperCliCredential','InteractiveBrowserCredential')
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $metadata -Encoding utf8NoBOM
$signtool = Join-Path $out 'sdk/bin/10.0.26100.0/x64/signtool.exe'
$dlib = Join-Path $out 'client/bin/x64/Azure.CodeSigning.Dlib.dll'
$manifest = Join-Path $out 'tools.json'
@{
    schema='vcp-signing-tools/1';packages=$specs;runtime=$runtime
    signtool=@{path=$signtool;sha256=(Get-FileHash -LiteralPath $signtool).Hash.ToLowerInvariant()}
    dlib=@{path=$dlib;sha256=(Get-FileHash -LiteralPath $dlib).Hash.ToLowerInvariant()}
    metadata=$metadata;metadata_sha256=(Get-FileHash -LiteralPath $metadata).Hash.ToLowerInvariant()
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifest -Encoding utf8NoBOM
Write-Output $manifest
