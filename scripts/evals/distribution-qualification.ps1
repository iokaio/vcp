# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$PackageResult, [string]$OutputRoot)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$receiptPath = [IO.Path]::GetFullPath($PackageResult)
$receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
$archive = Join-Path (Split-Path $receiptPath) $receipt.package
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $receipt.archive_sha256) { throw 'Exact archive digest changed' }
if (-not $OutputRoot) { $OutputRoot = Join-Path ([IO.Path]::GetTempPath()) ('vcp-distribution-qualification-' + [guid]::NewGuid()) }
$root = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $root) { throw 'New qualification directory required' }
New-Item -ItemType Directory -Path $root | Out-Null
$install = Join-Path $root 'Program Files café'; $data = Join-Path $root 'Local History'; $workspace = Join-Path $root 'Project 漢字'; $profile = Join-Path $root 'Empty Profile'
$staging = Join-Path $root 'Private Staging'; $vault = Join-Path $root 'Cloud Vault'
foreach ($directory in @($data,$workspace,$profile,$staging,$vault)) { New-Item -ItemType Directory -Path $directory | Out-Null }
$sentinels = @((Join-Path $data 'history.sentinel'),(Join-Path $data 'local-key.sentinel'),(Join-Path $workspace 'source.sentinel'),(Join-Path $vault 'ciphertext.sentinel'))
foreach ($file in $sentinels) { [IO.File]::WriteAllText($file, 'preserve-exact-synthetic-bytes') }
$bootstrap = Join-Path $root 'package-install.ps1'
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    $entry = @($zip.Entries | Where-Object FullName -ceq 'tools/package-install.ps1')
    if ($entry.Count -ne 1) { throw 'Packaged standalone installer missing' }
    [IO.Compression.ZipFileExtensions]::ExtractToFile($entry[0], $bootstrap, $false)
} finally { $zip.Dispose() }
$expectedInstaller = @($receipt.manifest.files | Where-Object path -ceq 'tools/package-install.ps1')
if ($expectedInstaller.Count -ne 1 -or (Get-FileHash -LiteralPath $bootstrap).Hash.ToLowerInvariant() -cne $expectedInstaller[0].sha256) { throw 'Bootstrap installer digest differs' }
$report = [ordered]@{
    schema = 'vcp-distribution-qualification/1'; status = 'running'; archive_sha256 = $receipt.archive_sha256
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription; architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    environment = 'Fresh process environment and empty profile directories on the current native host; not a separate OS account or machine'
    cases = @(); limitations = @('No second-machine recovery or owner acceptance', 'No signing or publication', 'No model inference in this offline package smoke test')
}
function Save-Report { $report | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8 }
function Invoke-Candidate([string]$Name,[string[]]$Arguments,[int]$ExpectedExit=0) {
    $info = [Diagnostics.ProcessStartInfo]::new($script:executable)
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true; $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $info.WorkingDirectory = $workspace; $info.Environment.Clear()
    foreach ($pair in @{SystemRoot=$env:SystemRoot; WINDIR=$env:WINDIR; PATH=(Join-Path $env:SystemRoot 'System32'); USERPROFILE=$profile; LOCALAPPDATA=$profile; APPDATA=$profile; TEMP=$profile; TMP=$profile}.GetEnumerator()) { $info.Environment[$pair.Key] = $pair.Value }
    foreach ($argument in @('--format','jsonl','--non-interactive','--workspace',$workspace,'--data-dir',$data)+$Arguments) { $info.ArgumentList.Add($argument) }
    $clock = [Diagnostics.Stopwatch]::StartNew(); $process = [Diagnostics.Process]::Start($info)
    $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
    $peak = $null
    while (-not $process.WaitForExit(10)) {
        $process.Refresh()
        try { $sample = $process.PeakWorkingSet64; if ($sample -gt $peak) { $peak = $sample } } catch { }
        if ($clock.ElapsedMilliseconds -ge 30000) { $process.Kill($true); $process.WaitForExit(); $process.Dispose(); throw "$Name exceeded native deadline" }
    }
    $out = $stdout.GetAwaiter().GetResult(); $err = $stderr.GetAwaiter().GetResult(); $clock.Stop()
    [IO.File]::WriteAllText((Join-Path $root "$Name.stdout.jsonl"), $out); [IO.File]::WriteAllText((Join-Path $root "$Name.stderr.txt"), $err)
    $code = $process.ExitCode
    $report.cases += @{name=$Name; exit_code=$code; expected_exit=$ExpectedExit; elapsed_ms=$clock.ElapsedMilliseconds; sampled_peak_working_set_bytes=$peak; cpu_ms=$process.TotalProcessorTime.TotalMilliseconds}
    $process.Dispose()
    Save-Report
    if ($code -ne $ExpectedExit) { throw "$Name returned unexpected exit ${code}: $err" }
    return $out
}
Save-Report
try {
    & pwsh -NoProfile -File $bootstrap -Action Install -PackageZip $archive -InstallRoot $install -DataRoot $data *> (Join-Path $root 'install.log')
    if ($LASTEXITCODE -ne 0) { throw 'Copied standalone installer failed' }
    $active = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    $script:executable = Join-Path $install "releases/$($active.release)/vcp.exe"
    $expectedBinary = @($receipt.manifest.files | Where-Object path -ceq 'vcp.exe')
    if ($expectedBinary.Count -ne 1 -or (Get-FileHash -LiteralPath $script:executable).Hash.ToLowerInvariant() -cne $expectedBinary[0].sha256) { throw 'Installed binary differs from packaged bytes' }
    $report.executable_sha256 = $expectedBinary[0].sha256
    $null = Invoke-Candidate 'help' @('--help')
    $null = Invoke-Candidate 'version' @('--version')
    $doctor = Invoke-Candidate 'doctor' @('doctor','--vault',$vault,'--staging',$staging)
    $frames = @($doctor -split '\r?\n' | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json })
    if (-not @($frames | Where-Object { $_.data.path_checks_passed -eq $true }).Count) { throw 'Separated native-path diagnosis did not pass' }
    $missing = Invoke-Candidate 'missing-profile' @('--config',(Join-Path $root 'absent-profile.json'),'run','Inspect synthetic source','--budget-usd','0.01','--autonomy','plan') 2
    if ([IO.File]::ReadAllText((Join-Path $root 'missing-profile.stderr.txt')) -notmatch 'configuration or data file is unavailable') { throw 'Missing profile did not produce the expected setup diagnostic' }
    if (@(Get-ChildItem -LiteralPath $data -Recurse -Filter canonical.frames).Count) { throw 'Missing profile unexpectedly created canonical work' }
    & pwsh -NoProfile -File $bootstrap -Action Uninstall -InstallRoot $install -DataRoot $data *> (Join-Path $root 'uninstall.log')
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $install)) { throw 'Native package uninstall failed' }
    foreach ($file in $sentinels) { if ([IO.File]::ReadAllText($file) -cne 'preserve-exact-synthetic-bytes') { throw 'Protected user-data sentinel changed' } }
    $report.sentinels_preserved = $true; $report.status = 'passed'
} catch { $report.status = 'failed'; $report.reason = $_.Exception.Message }
$report.ended_at = [DateTime]::UtcNow.ToString('o'); Save-Report
Write-Output (Join-Path $root 'result.json')
if ($report.status -ne 'passed') { exit 1 }
