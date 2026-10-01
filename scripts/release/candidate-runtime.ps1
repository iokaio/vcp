# SPDX-License-Identifier: Apache-2.0
# Shared only by explicit release smoke runners. Never uploaded with private state.
. (Join-Path $PSScriptRoot 'editor-layout.ps1')
function Invoke-BetaProcess([string]$Executable,[string[]]$Arguments,[string]$Directory,[hashtable]$Environment=@{},[int]$Seconds=180,[int]$Expected=0) {
    $info=[Diagnostics.ProcessStartInfo]::new($Executable)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.WorkingDirectory=$Directory
    $info.Environment.Clear()
    foreach ($name in @('SystemRoot','WINDIR','USERPROFILE','LOCALAPPDATA','APPDATA','TEMP','TMP','ProgramFiles','ProgramFiles(x86)')) {
        $value=[Environment]::GetEnvironmentVariable($name); if ($value) { $info.Environment[$name]=$value }
    }
    $info.Environment['PATH']="$env:SystemRoot\System32;$env:SystemRoot"
    foreach ($entry in $Environment.GetEnumerator()) { $info.Environment[$entry.Key]=$entry.Value }
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $capture=Join-Path $Directory ('process-'+[guid]::NewGuid())
    $outStream=[IO.File]::Create($capture+'.stdout'); $errStream=[IO.File]::Create($capture+'.stderr')
    $cancel=[Threading.CancellationTokenSource]::new()
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
    $stdout=$null; $stderr=$null; $started=$false; $cleanup=$true
    $watch=[Diagnostics.Stopwatch]::StartNew()
    try {
        $started=$process.Start(); if (-not $started) { throw 'Owned candidate process failed to start' }
        $stdout=$process.StandardOutput.BaseStream.CopyToAsync($outStream,81920,$cancel.Token)
        $stderr=$process.StandardError.BaseStream.CopyToAsync($errStream,81920,$cancel.Token)
        while (-not $process.WaitForExit(20)) {
            if ($watch.Elapsed.TotalSeconds -ge $Seconds -or $outStream.Length+$errStream.Length -gt 16MB) {
                $process.Kill($true)
                if (-not $process.WaitForExit(10000)) { throw 'Owned candidate process did not reap within 10 seconds' }
                throw 'Owned candidate process exceeded time or output limit'
            }
        }
        $null=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdout,$stderr)).WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
        if ($outStream.Length+$errStream.Length -gt 16MB) { throw 'Candidate output limit exceeded' }
        $outStream.Flush(); $errStream.Flush(); $outStream.Dispose(); $errStream.Dispose()
        $result=@{exit_code=$process.ExitCode;stdout=[IO.File]::ReadAllText($capture+'.stdout');stderr=[IO.File]::ReadAllText($capture+'.stderr')}
        if ($result.exit_code -ne $Expected) { throw "Candidate process returned $($result.exit_code), expected ${Expected}: $($result.stderr)" }
        return $result
    } finally {
        if ($started -and -not $process.HasExited) { $process.Kill($true); $cleanup=$process.WaitForExit(10000) }
        $copies=[Threading.Tasks.Task[]]@(@($stdout,$stderr) | Where-Object { $null -ne $_ })
        if (@($copies | Where-Object { -not $_.IsCompleted }).Count) {
            $cancel.Cancel(); $process.StandardOutput.Dispose(); $process.StandardError.Dispose()
            try { $null=[Threading.Tasks.Task]::WhenAll($copies).WaitAsync([TimeSpan]::FromSeconds(5)).GetAwaiter().GetResult() }
            catch { if (@($copies | Where-Object { -not $_.IsCompleted }).Count) { $cleanup=$false } }
        }
        foreach ($copy in $copies) { if ($copy.IsFaulted) { $null=$copy.Exception } }
        $outStream.Dispose(); $errStream.Dispose(); $cancel.Dispose(); $process.Dispose()
        if (-not $cleanup) { throw 'Candidate process supervision incomplete; private output retained' }
    }
}
function Install-BetaCandidate([string]$NativeResult,[string]$SetupResult,[string]$Root,[string]$Data) {
    $native=Get-Content -LiteralPath $NativeResult -Raw | ConvertFrom-Json
    $setup=Get-Content -LiteralPath $SetupResult -Raw | ConvertFrom-Json
    if ($native.status -cne 'release-candidate' -or $setup.schema -cne 'vcp-setup-result/1' -or $setup.candidate_id -cne $native.manifest.release.candidate_id -or $setup.native_archive_sha256 -cne $native.archive_sha256) { throw 'Strict matching native/setup candidate required' }
    if ($setup.archive.file -match '[\\/:]' -or $native.package -match '[\\/:]') { throw 'Invalid candidate archive basename' }
    $installer=Join-Path (Split-Path -Parent $SetupResult) $setup.archive.file
    $zip=Join-Path (Split-Path -Parent $NativeResult) $native.package
    if ((Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant() -cne $setup.archive.sha256 -or (Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $native.archive_sha256) { throw 'Candidate artifact bytes changed' }
    $registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1'
    if (Test-Path -LiteralPath $registration) { throw 'Existing registered VCP must be preserved; use a fresh test account' }
    if (Test-Path -LiteralPath $Root) { throw 'New smoke root required' }
    New-Item -ItemType Directory -Path $Root -Force | Out-Null
    $app=Join-Path $Root 'Program Files café'
    $null=Invoke-BetaProcess $installer @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',('/DIR='+$app),('/DATADIR='+$Data),'/TASKS=',('/LOG='+(Join-Path $Root 'install.log'))) $Root
    if (-not (Test-Path -LiteralPath $registration)) { throw 'Setup did not register per-user uninstall' }
    $registered=(Get-ItemProperty -LiteralPath $registration).InstallLocation
    if ([IO.Path]::GetFullPath($registered).TrimEnd('\') -ine [IO.Path]::GetFullPath($app).TrimEnd('\')) { throw 'Registered installation location differs' }
    $launcher=Join-Path $app 'vcp.exe'
    if ((Get-FileHash -LiteralPath $launcher).Hash.ToLowerInvariant() -cne $setup.launcher_sha256) { throw 'Installed launcher differs from receipt' }
    $selection=(Invoke-BetaProcess $launcher @('--resolve-installation') $Root).stdout | ConvertFrom-Json
    $expected=@($native.manifest.files | Where-Object path -ceq 'vcp.exe')
    $expectedEngine=Join-Path $app "engine/releases/$($native.archive_sha256)/vcp.exe"
    if ($selection.schema -cne 'vcp-installed-engine/1' -or $expected.Count -ne 1 -or [IO.Path]::GetFullPath($selection.executable).Replace('\\?\','') -ine $expectedEngine -or (Get-FileHash -LiteralPath $selection.executable).Hash.ToLowerInvariant() -cne $expected[0].sha256 -or [IO.Path]::GetFullPath($selection.data_directory).Replace('\\?\','').TrimEnd('\') -ine [IO.Path]::GetFullPath($Data).TrimEnd('\')) { throw 'Installed engine/data selection differs from final candidate' }
    return @{app=$app;launcher=$launcher;engine=$selection.executable;data=$Data;registration=$registration;native_sha256=$native.archive_sha256;setup_sha256=$setup.archive.sha256;engine_sha256=$expected[0].sha256}
}
function Wait-BetaUninstall([hashtable]$Installed,[ValidateRange(1,10)][int]$Seconds=10) {
    # Inno may finish deleting its own files shortly after the launched process
    # returns. Observe completion; never remove leftovers or retained data here.
    $watch=[Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        $registration=Test-Path -LiteralPath $Installed.registration -ErrorAction Stop
        $app=Test-Path -LiteralPath $Installed.app -ErrorAction Stop
        if (-not $registration -and -not $app) { return }
        if ($watch.Elapsed.TotalSeconds -ge $Seconds) {
            if ($registration) { throw 'Uninstall registration remains' }
            throw 'Owned installed program files remain after uninstall'
        }
        Start-Sleep -Milliseconds 100
    }
}
function Uninstall-BetaCandidate([hashtable]$Installed,[string]$Root) {
    $null=Invoke-BetaProcess (Join-Path $Installed.app 'unins000.exe') @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG='+(Join-Path $Root 'uninstall.log'))) $Root
    Wait-BetaUninstall $Installed
}
