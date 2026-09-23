# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageResult,
    [Parameter(Mandatory)][string]$PreviousPackageResult,
    [string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path ([IO.Path]::GetTempPath()) 'vcp-production-distribution' }
$root = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
# Actual CLI state must live outside repository/sync roots, as in ordinary use.
for ($ancestor = $root; $ancestor; $ancestor = Split-Path $ancestor -Parent) {
    if (Test-Path -LiteralPath (Join-Path $ancestor '.git')) { throw 'Qualification output with real CLI state must be outside repositories' }
    if ((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Qualification output ancestors must not be redirected' }
}
foreach ($name in @('OneDrive','OneDriveConsumer','OneDriveCommercial')) {
    $sync = [Environment]::GetEnvironmentVariable($name)
    if ($sync) {
        $sync = [IO.Path]::GetFullPath($sync).TrimEnd('\')
        if ($root -ieq $sync -or $root.StartsWith($sync + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Qualification output with real CLI state must be outside known sync roots' }
    }
}
New-Item -ItemType Directory -Path $root | Out-Null
$pwsh = (Get-Command pwsh -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$install = Join-Path $root 'Program Files café'
$data = Join-Path $root 'Local History'
$workspace = Join-Path $root 'Project 漢字'
$profile = Join-Path $root 'Empty Profile'
$vault = Join-Path $root 'Cloud Vault'
$staging = Join-Path $root 'Private Staging'
foreach ($directory in @($data,$workspace,$profile,$vault,$staging)) { New-Item -ItemType Directory -Path $directory | Out-Null }
$report = [ordered]@{
    schema = 'vcp-production-distribution-qualification/1'; status = 'running'
    started_at = [DateTime]::UtcNow.ToString('o')
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    pipe_drain_deadline_seconds = 10; pipe_cancel_deadline_seconds = 5; process_reap_deadline_seconds = 10
    cases = @(); assertions = @()
    limitations = @(
        'Fresh profile and environment on the current host, not a clean Windows OS or machine handoff.'
        'Compatible-format upgrade and rollback; no cross-format migration.'
        'State round trip covers real CLI storage preferences and protected sentinels, not model task history.'
        'Locked executable uses a native file handle; upgrade interruption uses the packaged before-activation hook.'
        'No model inference, owner approval, signing, release publication or physical full-volume exhaustion.'
    )
}
function Save-Report { $report | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8 }
function Assert-Case([string]$Name, [bool]$Condition) {
    $report.assertions += @{ name=$Name; passed=$Condition }; Save-Report
    if (-not $Condition) { throw "Assertion failed: $Name" }
}
function Read-Package([string]$ReceiptPath, [string]$Name) {
    $path = [IO.Path]::GetFullPath($ReceiptPath)
    $receipt = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    $archive = Join-Path (Split-Path $path) $receipt.package
    if ($receipt.archive_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        (Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $receipt.archive_sha256) { throw "$Name archive digest mismatch" }
    $binary = @($receipt.manifest.files | Where-Object path -ceq 'vcp.exe')
    $installer = @($receipt.manifest.files | Where-Object path -ceq 'tools/package-install.ps1')
    if ($binary.Count -ne 1 -or $installer.Count -ne 1) { throw "$Name executable or installer inventory missing" }
    $bootstrap = Join-Path $root "$Name-package-install.ps1"
    $buildDigest = $null
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $entry = @($zip.Entries | Where-Object FullName -ceq 'tools/package-install.ps1')
        if ($entry.Count -ne 1) { throw "$Name standalone installer missing" }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry[0], $bootstrap, $false)
        if ($Name -ceq 'production') {
            if ($receipt.manifest.build.status -cne 'recorded-local-build' -or $receipt.manifest.build.receipt -cne 'build-receipt.json') { throw 'Production package requires recorded build provenance' }
            $buildEntry = @($zip.Entries | Where-Object FullName -ceq 'build-receipt.json')
            $buildFile = @($receipt.manifest.files | Where-Object path -ceq 'build-receipt.json')
            if ($buildEntry.Count -ne 1 -or $buildFile.Count -ne 1 -or $buildEntry[0].Length -gt 32MB -or $buildEntry[0].Length -ne $buildFile[0].bytes) { throw 'Production build receipt inventory mismatch' }
            $inputStream = $buildEntry[0].Open()
            $buffer = [IO.MemoryStream]::new()
            try { $inputStream.CopyTo($buffer); $buildBytes = $buffer.ToArray() } finally { $inputStream.Dispose(); $buffer.Dispose() }
            $buildDigest = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($buildBytes)).ToLowerInvariant()
            if ($buildDigest -cne $buildFile[0].sha256 -or $buildDigest -cne $receipt.manifest.build.receipt_sha256) { throw 'Production build receipt digest mismatch' }
            $build = [Text.Encoding]::UTF8.GetString($buildBytes).TrimStart([char]0xFEFF) | ConvertFrom-Json
            if ($build.schema -cne 'vcp-local-build/1' -or $build.exit_code -ne 0 -or $build.executable_sha256 -cne $binary[0].sha256 -or
                $build.profile -cne 'release' -or $build.qualification_build -ne $false -or $build.source_stable -ne $true -or $build.target -cne 'x86_64-pc-windows-msvc') {
                throw 'Production candidate requires a successful stable native release build without qualification features'
            }
        }
    } finally { $zip.Dispose() }
    if ((Get-FileHash -LiteralPath $bootstrap).Hash.ToLowerInvariant() -cne $installer[0].sha256) { throw "$Name installer digest mismatch" }
    return @{ receipt=$path; archive=$archive; archive_sha256=$receipt.archive_sha256; executable_sha256=$binary[0].sha256; build_receipt_sha256=$buildDigest; installer=$bootstrap; manifest=$receipt.manifest }
}
function Invoke-Bounded([string]$Name, [string]$Program, [string[]]$Arguments, [bool]$ExpectSuccess=$true, [bool]$CleanEnvironment=$false, [string]$Fault='', [int]$DeadlineSeconds=120) {
    $info = [Diagnostics.ProcessStartInfo]::new($Program)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.RedirectStandardInput=$true
    $info.WorkingDirectory=$workspace
    if ($CleanEnvironment) {
        $info.Environment.Clear()
        foreach ($pair in @{SystemRoot=$env:SystemRoot; WINDIR=$env:WINDIR; PATH=(Join-Path $env:SystemRoot 'System32'); USERPROFILE=$profile; LOCALAPPDATA=$profile; APPDATA=$profile; TEMP=$profile; TMP=$profile}.GetEnumerator()) { $info.Environment[$pair.Key]=$pair.Value }
    }
    $null = $info.Environment.Remove('VCP_PACKAGE_INSTALL_FAULT')
    if ($Fault) { $info.Environment['VCP_PACKAGE_INSTALL_FAULT']=$Fault }
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $outPath=Join-Path $root "$Name.stdout.txt"; $errPath=Join-Path $root "$Name.stderr.txt"
    $outStream=[IO.File]::Create($outPath); $errStream=[IO.File]::Create($errPath)
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
    $pipeCancellation=[Threading.CancellationTokenSource]::new()
    $stdout=$null; $stderr=$null; $started=$false; $reaped=$false; $pipeCleanupComplete=$true
    $timedOut=$false; $peak=0; $cpu=$null; $exitCode=$null; $supervisionError=$null; $drainError=$null; $drainTimedOut=$false
    $clock=[Diagnostics.Stopwatch]::StartNew(); $drainClock=[Diagnostics.Stopwatch]::new()
    try {
        $started=$process.Start()
        if (-not $started) { throw 'Process start failed' }
        $process.StandardInput.Close()
        $stdout=$process.StandardOutput.BaseStream.CopyToAsync($outStream,81920,$pipeCancellation.Token)
        $stderr=$process.StandardError.BaseStream.CopyToAsync($errStream,81920,$pipeCancellation.Token)
        while (-not $process.WaitForExit(20)) {
            $process.Refresh()
            try { $peak=[Math]::Max($peak,$process.PeakWorkingSet64) } catch { }
            if ($clock.Elapsed.TotalSeconds -ge $DeadlineSeconds) {
                $timedOut=$true; $process.Kill($true)
                if (-not $process.WaitForExit(10000)) { throw 'Timed-out process did not reap within 10 seconds' }
                break
            }
        }
        $clock.Stop(); $reaped=$process.HasExited; $drainClock.Start()
        try {
            $copies=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdout,$stderr))
            $null=$copies.WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
        }
        catch [TimeoutException] { $drainTimedOut=$true; $drainError='Output pipes did not finish within the 10-second drain deadline' }
        catch { $drainError=$_.Exception.Message }
        $drainClock.Stop(); $exitCode=$process.ExitCode; $cpu=$process.TotalProcessorTime.TotalMilliseconds
    } catch { $supervisionError=$_.Exception.Message }
    finally {
        $clock.Stop(); $drainClock.Stop()
        if ($started) {
            try {
                if (-not $process.HasExited) { $process.Kill($true); $reaped=$process.WaitForExit(10000) }
                else { $reaped=$true }
            } catch { $supervisionError=$_.Exception.Message; $reaped=$false }
        }
        $copyTasks=[Threading.Tasks.Task[]]@(@($stdout,$stderr) | Where-Object { $null -ne $_ })
        if (@($copyTasks | Where-Object { -not $_.IsCompleted }).Count) {
            $pipeCancellation.Cancel()
            if ($started) { $process.StandardOutput.Dispose(); $process.StandardError.Dispose() }
            try { $null=[Threading.Tasks.Task]::WhenAll($copyTasks).WaitAsync([TimeSpan]::FromSeconds(5)).GetAwaiter().GetResult() }
            catch { if (@($copyTasks | Where-Object { -not $_.IsCompleted }).Count) { $pipeCleanupComplete=$false } }
        }
        foreach ($copyTask in $copyTasks) { if ($copyTask.IsFaulted) { $null=$copyTask.Exception } }
        $outStream.Dispose(); $errStream.Dispose(); $pipeCancellation.Dispose(); $process.Dispose()
    }
    $out=[IO.File]::ReadAllText($outPath); $err=[IO.File]::ReadAllText($errPath)
    $passed=($started -and $reaped -and $pipeCleanupComplete -and -not $timedOut -and -not $supervisionError -and -not $drainError -and -not $drainTimedOut -and (($exitCode -eq 0) -eq $ExpectSuccess))
    $report.cases += @{
        name=$Name; program=$Program; arguments=$Arguments; expected_success=$ExpectSuccess
        exit_code=$exitCode; passed=$passed; timed_out=$timedOut; deadline_seconds=$DeadlineSeconds
        elapsed_ms=$clock.ElapsedMilliseconds; cpu_ms=$cpu; sampled_peak_working_set_bytes=if ($peak -gt 0) { $peak } else { $null }
        process_reaped=$reaped; pipe_cleanup_complete=$pipeCleanupComplete; supervision_error=$supervisionError
        drain_timed_out=$drainTimedOut; drain_error=$drainError; drain_ms=$drainClock.Elapsed.TotalMilliseconds
        unresolved_pipe_holder=($drainTimedOut -and $reaped)
        stdout_sha256=(Get-FileHash -LiteralPath $outPath).Hash.ToLowerInvariant()
        stderr_sha256=(Get-FileHash -LiteralPath $errPath).Hash.ToLowerInvariant()
    }
    Save-Report
    if (-not $passed) { throw "$Name failed or supervision incomplete; retained output: $errPath" }
    return @{stdout=$out; stderr=$err}
}
function Installer([string]$Name,[string]$Action,$Package,[bool]$Success=$true,[string]$Fault='',[string]$State='') {
    $installerPath = if ($Action -ceq 'Install' -and $Package) { $Package.installer } else { $candidate.installer }
    $arguments=@('-NoProfile','-File',$installerPath,'-Action',$Action,'-InstallRoot',$install,'-DataRoot',$data)
    if ($Package) { $arguments+=@('-PackageZip',$Package.archive) }
    if ($State) { $arguments+=@('-StateManifest',$State) }
    return Invoke-Bounded $Name $pwsh $arguments $Success $false $Fault
}
function Active($Package) {
    $active=Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    Assert-Case "active-$($Package.archive_sha256)" ($active.release -ceq $Package.archive_sha256 -and $active.package_sha256 -ceq $Package.archive_sha256)
    $binary=Join-Path $install "releases/$($active.release)/vcp.exe"
    Assert-Case "installed-$($Package.executable_sha256)" ((Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant() -ceq $Package.executable_sha256)
    return $binary
}
function Invoke-InstalledCli([string]$Name,[string]$Binary,[string[]]$Arguments) {
    return Invoke-Bounded $Name $Binary (@('--format','jsonl','--non-interactive','--workspace',$workspace,'--data-dir',$data)+$Arguments) $true $true '' 30
}
function Result-Data($Output) {
    $rows=@($Output.stdout -split '\r?\n' | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json })
    $results=@($rows | Where-Object type -ceq 'result')
    if ($results.Count -ne 1) { throw 'Expected one CLI result frame' }
    return $results[0].data
}
Save-Report
try {
    $candidate=Read-Package $PackageResult 'production'
    $previous=Read-Package $PreviousPackageResult 'previous'
    if ($candidate.archive_sha256 -ceq $previous.archive_sha256) { throw 'Distinct real package identities required' }
    $report.candidate=$candidate; $report.previous=$previous; Save-Report
    # The existing smoke owns a separate fresh installation and empty environment.
    $null=Invoke-Bounded 'fresh-profile-smoke' $pwsh @('-NoProfile','-File',(Join-Path $PSScriptRoot 'distribution-qualification.ps1'),'-PackageResult',$candidate.receipt,'-OutputRoot',(Join-Path $root 'fresh-profile')) $true $false '' 180
    $smokePath=Join-Path $root 'fresh-profile/result.json'
    $smoke=Get-Content -LiteralPath $smokePath -Raw | ConvertFrom-Json
    Assert-Case 'fresh-profile-exact-production' ($smoke.status -ceq 'passed' -and $smoke.archive_sha256 -ceq $candidate.archive_sha256 -and $smoke.executable_sha256 -ceq $candidate.executable_sha256)
    $report.fresh_profile_receipt_sha256=(Get-FileHash -LiteralPath $smokePath).Hash.ToLowerInvariant()
    $sentinels=@((Join-Path $data 'history.sentinel'),(Join-Path $data 'local-key.sentinel'),(Join-Path $workspace 'source.sentinel'),(Join-Path $vault 'ciphertext.sentinel'))
    foreach ($file in $sentinels) { [IO.File]::WriteAllText($file,'preserve-exact-synthetic-bytes') }
    $null=Installer 'install-previous' 'Install' $previous
    $oldBinary=Active $previous
    $null=Invoke-InstalledCli 'previous-configure' $oldBinary @('storage','configure','--backend','files')
    $oldState=Result-Data (Invoke-InstalledCli 'previous-state' $oldBinary @('storage','configure','--backend','files','--preview'))
    $statePath=Join-Path $root 'incompatible-state.json'
    @{compatibility=@{canonical='intentionally-unsupported-format';config=$candidate.manifest.compatibility.config;index=$candidate.manifest.compatibility.index}} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $statePath -Encoding utf8
    $refused=Installer 'incompatible-upgrade' 'Upgrade' $candidate $false '' $statePath
    Assert-Case 'incompatible-upgrade-diagnostic' ($refused.stderr -match 'Incompatible or unspecified canonical format')
    $null=Active $previous
    $interrupted=Installer 'interrupted-upgrade' 'Upgrade' $candidate $false 'before-activation'
    Assert-Case 'upgrade-interruption-diagnostic' ($interrupted.stderr -match 'Deterministic fault before active pointer replacement')
    $null=Active $previous
    $afterFault=Result-Data (Invoke-InstalledCli 'previous-after-interruption' $oldBinary @('storage','configure','--backend','files','--preview'))
    Assert-Case 'previous-state-after-interruption' (($oldState | ConvertTo-Json -Depth 20 -Compress) -ceq ($afterFault | ConvertTo-Json -Depth 20 -Compress))
    $null=Installer 'retry-upgrade' 'Upgrade' $candidate
    $newBinary=Active $candidate
    $newState=Result-Data (Invoke-InstalledCli 'production-reads-previous-state' $newBinary @('storage','configure','--backend','files','--preview'))
    Assert-Case 'production-reads-compatible-state' (($oldState | ConvertTo-Json -Depth 20 -Compress) -ceq ($newState | ConvertTo-Json -Depth 20 -Compress))
    $null=Invoke-InstalledCli 'production-writes-state' $newBinary @('storage','configure','--backend','sqlite','--expected-revision',([string]$newState.preference_revision))
    $writtenState=Result-Data (Invoke-InstalledCli 'production-state' $newBinary @('storage','configure','--backend','sqlite','--preview'))
    Assert-Case 'production-advances-state-revision' ($writtenState.preference_revision -eq ($oldState.preference_revision + 1))
    $refused=Installer 'incompatible-rollback' 'Rollback' $null $false '' $statePath
    Assert-Case 'incompatible-rollback-diagnostic' ($refused.stderr -match 'Incompatible or unspecified canonical format')
    $null=Active $candidate
    $lock=[IO.File]::Open($newBinary,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        $null=Installer 'locked-uninstall' 'Uninstall' $null $false
        Assert-Case 'locked-uninstall-preserves-ownership' (Test-Path -LiteralPath (Join-Path $install '.vcp-install-owned.json'))
        $null=Active $candidate
        $null=Installer 'rollback-while-production-locked' 'Rollback' $null
    } finally { $lock.Dispose() }
    $oldBinary=Active $previous
    $rolledState=Result-Data (Invoke-InstalledCli 'previous-reads-production-state' $oldBinary @('storage','configure','--backend','sqlite','--preview'))
    Assert-Case 'previous-reads-production-written-state' (($writtenState | ConvertTo-Json -Depth 20 -Compress) -ceq ($rolledState | ConvertTo-Json -Depth 20 -Compress))
    $preferenceFiles=@(Get-ChildItem -LiteralPath $data -Recurse -File -Filter storage-preference.json)
    Assert-Case 'real-storage-preference-retained' ($preferenceFiles.Count -eq 1)
    $preferenceHash=(Get-FileHash -LiteralPath $preferenceFiles[0].FullName).Hash.ToLowerInvariant()
    $null=Installer 'uninstall' 'Uninstall' $null
    Assert-Case 'owned-installation-removed' (-not (Test-Path -LiteralPath $install))
    Assert-Case 'real-state-preserved-after-uninstall' ((Get-FileHash -LiteralPath $preferenceFiles[0].FullName).Hash.ToLowerInvariant() -ceq $preferenceHash)
    foreach ($file in $sentinels) { Assert-Case ('preserved-'+(Split-Path $file -Leaf)) ([IO.File]::ReadAllText($file) -ceq 'preserve-exact-synthetic-bytes') }
    foreach ($package in @($previous,$candidate)) { Assert-Case 'archive-unchanged' ((Get-FileHash -LiteralPath $package.archive).Hash.ToLowerInvariant() -ceq $package.archive_sha256) }
    $report.status='passed'
} catch {
    $report.status='failed'; $report.reason=$_.Exception.Message
} finally {
    $report.ended_at=[DateTime]::UtcNow.ToString('o'); Save-Report
}
Write-Output (Join-Path $root 'result.json')
if ($report.status -ne 'passed') { exit 1 }
