# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Executes one exact ignored test against independently pinned local inputs.
# Source/asset manifests contain files: [{path, sha256}]; full logs remain private.
param(
 [Parameter(Mandatory)][string]$TestExecutable,
 [Parameter(Mandatory)][string]$TestSha256,
 [Parameter(Mandatory)][string]$Executable,
 [Parameter(Mandatory)][string]$ExpectedSha256,
 [Parameter(Mandatory)][string]$Assets,
 [Parameter(Mandatory)][string]$AssetsManifest,
 [Parameter(Mandatory)][string]$AssetsManifestSha256,
 [Parameter(Mandatory)][string]$GitExecutable,
 [Parameter(Mandatory)][string]$GitSha256,
 [Parameter(Mandatory)][string]$SourceIdentityManifest,
 [Parameter(Mandatory)][string]$SourceIdentitySha256,
 [Parameter(Mandatory)][string]$SourceRoot,
 [Parameter(Mandatory)][string]$OutputRoot
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Native Windows required' }
function Plain([string]$Path) {
 $full=[IO.Path]::GetFullPath($Path)
 if ($full.StartsWith('\\') -or -not [IO.Path]::IsPathFullyQualified($full)) { throw 'Local absolute path required' }
 for ($entry=[IO.FileInfo]::new($full); $null -ne $entry; $entry=if ($entry -is [IO.DirectoryInfo]) {$entry.Parent} else {$entry.Directory}) {
  if ($entry.Exists -and ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse path rejected' }
 }
 return $full
}
function Checked([string]$Path,[string]$Expected) {
 $resolved=Plain $Path
 if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) { throw 'Regular input file required' }
 if ($Expected -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash -ine $Expected) { throw 'Pinned input hash mismatch' }
 return $resolved
}
$test=Checked $TestExecutable $TestSha256
$exe=Checked $Executable $ExpectedSha256
$git=Checked $GitExecutable $GitSha256
$manifestPath=Checked $AssetsManifest $AssetsManifestSha256
$sourceManifest=Checked $SourceIdentityManifest $SourceIdentitySha256
$sourceBase=Plain $SourceRoot
if (-not (Test-Path -LiteralPath $sourceBase -PathType Container)) { throw 'Source directory required' }
$sourceBase=$sourceBase.TrimEnd('\')+'\'
$sourceFiles=Get-Content -LiteralPath $sourceManifest -Raw | ConvertFrom-Json
if (@($sourceFiles.files).Count -eq 0) { throw 'Empty source identity manifest' }
function Check-Source {
 foreach ($row in $sourceFiles.files) {
  $path=[IO.Path]::GetFullPath((Join-Path $sourceBase $row.path))
  if (-not $path.StartsWith($sourceBase,[StringComparison]::OrdinalIgnoreCase)) { throw 'Source path escapes bound root' }
  $null=Checked $path $row.sha256
 }
}
Check-Source
$assetRoot=Plain $Assets
if (-not (Test-Path -LiteralPath $assetRoot -PathType Container)) { throw 'Asset directory required' }
$assetPrefix=$assetRoot.TrimEnd('\')+'\'
$manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if (@($manifest.files).Count -eq 0) { throw 'Empty pinned assets manifest' }
function Check-Assets {
 foreach ($row in $manifest.files) {
  $path=[IO.Path]::GetFullPath((Join-Path $assetRoot $row.path))
  if (-not $path.StartsWith($assetPrefix,[StringComparison]::OrdinalIgnoreCase)) { throw 'Asset path escapes pinned root' }
  $null=Checked $path $row.sha256
 }
}
Check-Assets
$OutputRoot=Plain $OutputRoot
if (Test-Path -LiteralPath $OutputRoot) { throw 'OutputRoot must be fresh' }
$out=(New-Item -ItemType Directory -Path $OutputRoot).FullName
$scriptHash=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
$env:VCP_TEST_PRODUCTION_BINARY=$exe
$env:VCP_MINILM_ASSETS=$assetRoot
$env:VCP_TEST_NATIVE_GIT=$git
$name='memory_restore::production_memory_optimizer_exclusion_survive_encrypted_cross_backend_restore'
$process=Start-Process -FilePath $test -ArgumentList @('--ignored','--exact',$name,'--nocapture','--test-threads=1') -WorkingDirectory $out -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $out 'stdout.log') -RedirectStandardError (Join-Path $out 'stderr.log')
$finished=$process.WaitForExit(1800000)
if (-not $finished) { $process.Kill(); $process.WaitForExit(); throw 'Campaign exceeded 30 minutes; investigate any surviving owned child before retry' }
$process.WaitForExit()
$stdout=Get-Content -LiteralPath (Join-Path $out 'stdout.log') -Raw
$stderr=Get-Content -LiteralPath (Join-Path $out 'stderr.log') -Raw
$observed=($stdout -match 'test result: ok\. 1 passed; 0 failed; 0 ignored;' -and
 $stderr -match 'production-memory-restore from=Files to=Sqlite passed; provider_calls=0' -and
 $stderr -match 'production-memory-restore from=Sqlite to=Files passed; provider_calls=0')
$null=Checked $test $TestSha256
$null=Checked $exe $ExpectedSha256
$null=Checked $git $GitSha256
$null=Checked $manifestPath $AssetsManifestSha256
$null=Checked $sourceManifest $SourceIdentitySha256
Check-Source
Check-Assets
$null=Checked $PSCommandPath $scriptHash
$receipt=@{schema='vcp-production-memory-restore/1';test=$name;exit_code=$process.ExitCode;passed=($process.ExitCode -eq 0 -and $observed);completed_at=[DateTime]::UtcNow.ToString('o');script_sha256=$scriptHash;production_sha256=$ExpectedSha256;test_sha256=$TestSha256;git_sha256=$GitSha256;assets_manifest_sha256=$AssetsManifestSha256;source_identity_manifest_sha256=$SourceIdentitySha256;stdout_sha256=(Get-FileHash -LiteralPath (Join-Path $out 'stdout.log') -Algorithm SHA256).Hash.ToLowerInvariant();stderr_sha256=(Get-FileHash -LiteralPath (Join-Path $out 'stderr.log') -Algorithm SHA256).Hash.ToLowerInvariant();provider_calls=0;directions=@('files-to-sqlite','sqlite-to-files')}
$receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $out 'result.json') -Encoding utf8
if ($process.ExitCode -ne 0 -or -not $observed) { throw 'Production memory restore qualification failed or exact test did not run; inspect retained logs' }
