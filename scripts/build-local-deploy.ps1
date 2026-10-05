# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
<#
.SYNOPSIS
Rebuild the current checkout and deploy it to an existing VCP installation for local testing.
.EXAMPLE
.\scripts\build-local-deploy.ps1 -Jobs 16
.EXAMPLE
.\scripts\build-local-deploy.ps1 -UseLatestBuild
.EXAMPLE
.\scripts\build-local-deploy.ps1 -InstallRoot 'C:\Users\me\AppData\Local\Programs\VCP' -WhatIf
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$InstallRoot,
    [switch]$UseLatestBuild,
    [string]$BuildRoot = (Join-Path $PSScriptRoot '../artifacts/local-setup'),
    [ValidateRange(1,256)][int]$Jobs = [int][Math]::Max(1, [Math]::Min(32, [Math]::Min(
        [Environment]::ProcessorCount - 1,
        [Math]::Floor([GC]::GetGCMemoryInfo().TotalAvailableMemoryBytes / 4GB))))
)
$ErrorActionPreference = 'Stop'

function Assert-LocalDeployPath([string]$Path, [switch]$Directory) {
    if ($Path -notmatch '^[a-zA-Z]:[\\/]' -or $Path.Split([char[]]'\/') -contains '..') {
        throw 'Local deployment requires an absolute local drive path without parent traversal'
    }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    if ($full.Length -lt 4) { throw 'A drive root cannot be a deployment path' }
    for ($cursor = $full; $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Redirected deployment path rejected: $cursor" }
            if ($cursor -ine $full -and -not $item.PSIsContainer) { throw 'Deployment ancestor must be a directory' }
        }
    }
    $item = Get-Item -LiteralPath $full -Force
    if ($item.PSIsContainer -ne [bool]$Directory) { throw 'Deployment input has the wrong file type' }
    if ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($full)).DriveType -eq [IO.DriveType]::Network) { throw 'Network deployment paths are unsupported' }
    return $full
}
function Read-LocalDeployJson([string]$Path) {
    $file = Assert-LocalDeployPath $Path
    if ((Get-Item -LiteralPath $file).Length -gt 16MB) { throw 'Deployment metadata exceeds size limit' }
    return Get-Content -LiteralPath $file -Raw | ConvertFrom-Json -Depth 100
}
function Get-LocalDeployInstallation([string]$Requested) {
    if (-not $Requested) {
        $candidates = @(foreach ($hive in 'HKCU','HKLM') {
            $record = Get-ItemProperty -LiteralPath "${hive}:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1" -ErrorAction SilentlyContinue
            if ($record.InstallLocation) { [IO.Path]::GetFullPath($record.InstallLocation).TrimEnd('\') }
        }) | Select-Object -Unique
        if (@($candidates).Count -gt 1) { throw 'Multiple registered VCP installations found; select one with -InstallRoot' }
        if (@($candidates).Count -eq 1) { $Requested = [string]$candidates }
        else {
            $command = Get-Command vcp -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($command) { $Requested = Split-Path -Parent $command.Source }
        }
    }
    if (-not $Requested) { throw 'No existing VCP installation found; provide -InstallRoot with the installed application directory' }
    $app = Assert-LocalDeployPath $Requested -Directory
    $engine = Assert-LocalDeployPath (Join-Path $app 'engine') -Directory
    $launcher = Assert-LocalDeployPath (Join-Path $app 'vcp.exe')
    $owner = Read-LocalDeployJson (Join-Path $engine '.vcp-install-owned.json')
    $pointer = Read-LocalDeployJson (Join-Path $engine 'active.json')
    if ($owner.install_root -ine $engine -or $pointer.release -cnotmatch '^[a-f0-9]{64}$' -or $pointer.package_sha256 -cne $pointer.release) {
        throw 'Existing installation ownership or active release is invalid'
    }
    $scope = if ($owner.schema -ceq 'vcp-install-owned/1' -and $pointer.schema -ceq 'vcp-install-pointer/1' -and
        $owner.data_root -and $pointer.data_root -ceq $owner.data_root -and -not $owner.data_scope -and -not $pointer.data_scope) { 'Private' }
    elseif ($owner.schema -ceq 'vcp-install-owned/2' -and $pointer.schema -ceq 'vcp-install-pointer/2' -and
        $owner.data_scope -ceq 'user' -and $pointer.data_scope -ceq 'user' -and -not $owner.data_root -and -not $pointer.data_root) { 'User' }
    else { throw 'Existing installation data scope is invalid' }
    if ($scope -eq 'Private') {
        if ($owner.data_root -notmatch '^[a-zA-Z]:[\\/]') { throw 'Installed data root must be an absolute local path' }
        $data = [IO.Path]::GetFullPath($owner.data_root).TrimEnd('\')
        if ($data -ieq $app -or $data.StartsWith($app + '\',[StringComparison]::OrdinalIgnoreCase) -or $app.StartsWith($data + '\',[StringComparison]::OrdinalIgnoreCase)) {
            throw 'Installed application and protected data must be disjoint'
        }
    }
    $null = Assert-LocalDeployPath (Join-Path $engine "releases/$($pointer.release)/vcp.exe")
    return [pscustomobject]@{ App=$app; Engine=$engine; Launcher=$launcher; Owner=$owner; Pointer=$pointer; Scope=$scope }
}
function Get-LocalDeployCompletedBuild([string]$Root) {
    . (Join-Path $PSScriptRoot 'install-local.ps1')
    $selected=Get-LatestLocalInstaller $Root
    $setup=Read-LocalDeployJson $selected.RecordPath
    if ($setup.native_result -cne 'native-result.json') { throw 'Completed build must identify its adjacent native result' }
    $resultPath=Join-Path (Split-Path -Parent $selected.RecordPath) $setup.native_result
    $native=Read-LocalDeployJson $resultPath
    $version=$selected.Version.ToString()
    $identity=$native.manifest.local_candidate
    if ($native.schema -cne 'vcp-distribution-result/1' -or $native.status -cne 'candidate' -or
        $native.package -cne "vcp-$version-windows-x64-unsigned-local.zip" -or
        $native.archive_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $native.archive_sha256 -cne $setup.native_archive_sha256 -or
        $native.manifest.artifact -cne 'unsigned-local-candidate' -or $native.manifest.release -or
        $native.manifest.signing.status -cne 'unsigned' -or
        $identity.schema -cne 'vcp-local-candidate/1' -or $identity.native_version -cne $version -or
        $identity.candidate_id -cnotmatch '^[a-f0-9]{64}$' -or $identity.candidate_id -cne $setup.candidate_id -or $identity.signing -cne 'unsigned') {
        throw 'Completed setup and native payload identities differ'
    }
    $receiptPath=Join-Path (Split-Path -Parent $resultPath) 'package/build-receipt.json'
    $receipt=Read-LocalDeployJson $receiptPath
    if ((Get-FileHash -LiteralPath $receiptPath).Hash.ToLowerInvariant() -cne $setup.build_receipt_sha256 -or
        $receipt.launcher_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $receipt.launcher_sha256 -cne $setup.launcher_sha256 -or
        $receipt.schema -cne 'vcp-local-build/1' -or $receipt.exit_code -ne 0 -or $receipt.cargo_exit_code -ne 0 -or
        $receipt.source_stable -ne $true -or $receipt.toolchain_stable -ne $true -or $receipt.qualification_build -ne $false -or $receipt.release) {
        throw 'Completed payload build receipt differs from its recorded build'
    }
    return [pscustomobject]@{Version=$version;PackageResult=$resultPath;ReceiptPath=$receiptPath}
}
function Invoke-LocalDeployExecutable([string]$File, [string[]]$Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new($File)
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $start.StandardOutputEncoding=[Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new(); $process.StartInfo=$start
    try {
        if (-not $process.Start()) { throw 'Cannot start native deployment verification' }
        $stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { $process.Kill($true); $process.WaitForExit(); throw 'Native deployment verification timed out' }
        if ($process.ExitCode -ne 0) { throw "Native deployment verification failed: $($stderr.GetAwaiter().GetResult())" }
        return $stdout.GetAwaiter().GetResult().Trim()
    } finally { $process.Dispose() }
}
function Assert-LocalDeploySelection($Installation, [string]$ExpectedEngine) {
    $selection = Invoke-LocalDeployExecutable $Installation.Launcher @('--resolve-installation') | ConvertFrom-Json
    if ($selection.schema -cne 'vcp-installed-engine/1' -or
        [IO.Path]::GetFullPath($selection.executable).Replace('\\?\','') -ine $ExpectedEngine) { throw 'Installed launcher selected a different native engine' }
    if ($Installation.Scope -eq 'Private' -and [IO.Path]::GetFullPath($selection.data_directory).Replace('\\?\','').TrimEnd('\') -ine $Installation.Owner.data_root.TrimEnd('\')) {
        throw 'Installed launcher changed the protected data directory'
    }
    return $selection
}
function Enter-LocalDeployMutex([string]$Name) {
    $mutex=[Threading.Mutex]::new($false,$Name)
    try {
        try { $owned=$mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $owned=$true }
        if (-not $owned) { throw 'Another VCP installation operation is in progress; wait for it to finish' }
        return $mutex
    } catch { $mutex.Dispose(); throw }
}
function Replace-LocalDeployFile([string]$Source, [string]$Destination) {
    $null=Assert-LocalDeployPath $Source
    $null=Assert-LocalDeployPath $Destination
    $temporary=Join-Path (Split-Path -Parent $Destination) ('.local-deploy-' + [guid]::NewGuid().ToString('N'))
    try {
        Copy-Item -LiteralPath $Source -Destination $temporary
        [IO.File]::Replace($temporary,$Destination,[NullString]::Value,$true)
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
}
function Invoke-LocalDeployBuild([string]$BuildScript, [string]$OutputRoot, [int]$BuildJobs) {
    $flags=[Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')
    $recipe=@('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31
    try {
        if ($flags -ceq $recipe) { [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',[NullString]::Value,'Process') }
        & $BuildScript -OutputRoot $OutputRoot -Jobs $BuildJobs
    } finally {
        if ($flags -ceq $recipe) { [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$flags,'Process') }
    }
}
function Invoke-LocalDeployPayload($Installation, [string]$PackageResult, [string]$EvidenceRoot, [string]$Version) {
    $setupLock=$null; $installLock=$null; $changed=$false
    $backupLauncher=Join-Path $EvidenceRoot 'previous-launcher.exe'
    $backupPointer=Join-Path $EvidenceRoot 'previous-active.json'
    try {
        $setupName=if($Installation.Scope -eq 'User'){'Global\VCP.Setup.Shared'}else{'Global\VCP.Setup.' + [Environment]::MachineName.ToUpperInvariant() + '.' + [Environment]::UserName.ToUpperInvariant()}
        $setupLock=Enter-LocalDeployMutex $setupName
        $identity=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $installLock=Enter-LocalDeployMutex $(if($Installation.Scope -eq 'User'){'Global\VCP.Install.Shared'}else{"Global\VCP.Install.$identity"})
        $Installation=Get-LocalDeployInstallation $Installation.App
        $oldEngine=Join-Path $Installation.Engine "releases/$($Installation.Pointer.release)/vcp.exe"
        $oldSelection=Assert-LocalDeploySelection $Installation $oldEngine
        foreach ($process in Get-Process) {
            $path=$process.Path
            if ($path -and $path.StartsWith($Installation.App + '\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Close VCP CLI and editor connections before deploying local binaries' }
        }
        $result=Read-LocalDeployJson $PackageResult
        if ($result.schema -cne 'vcp-distribution-result/1' -or $result.status -cne 'candidate' -or
            $result.package -cnotin @('vcp-windows-unsigned.zip',"vcp-$Version-windows-x64-unsigned-local.zip") -or
            $result.archive_sha256 -cnotmatch '^[a-f0-9]{64}$' -or $result.manifest.release) { throw 'Expected an unsigned local native payload' }
        $packageRoot=Split-Path -Parent $PackageResult
        $archive=Assert-LocalDeployPath (Join-Path $packageRoot $result.package)
        if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $result.archive_sha256) { throw 'Native payload archive changed' }
        $launcher=Assert-LocalDeployPath (Join-Path $packageRoot 'package/vcp-launch.exe')
        $launcherHash=(Get-FileHash -LiteralPath $launcher).Hash.ToLowerInvariant()
        $launcherEntries=@($result.manifest.files | Where-Object path -ceq 'vcp-launch.exe')
        $payloadReceipt=Read-LocalDeployJson (Join-Path $packageRoot 'package/build-receipt.json')
        if ($launcherEntries.Count -ne 1 -or $launcherEntries[0].sha256 -cne $launcherHash -or
            $launcherEntries[0].bytes -ne (Get-Item -LiteralPath $launcher).Length -or $payloadReceipt.launcher_sha256 -cne $launcherHash) {
            throw 'Staged launcher differs from the native payload or build receipt'
        }
        if ((Invoke-LocalDeployExecutable $launcher @('--launcher-version')) -cne "vcp-launch $Version") { throw 'Built launcher version differs from current source' }
        Copy-Item -LiteralPath $Installation.Launcher -Destination $backupLauncher
        Copy-Item -LiteralPath (Join-Path $Installation.Engine 'active.json') -Destination $backupPointer
        $installArgs=@{Action='Upgrade';PackageZip=$archive;InstallRoot=$Installation.Engine;DataScope=$Installation.Scope}
        if ($Installation.Scope -eq 'Private') { $installArgs.DataRoot=$Installation.Owner.data_root }
        # Same-thread invocation keeps the existing installation mutex reentrant.
        $changed=$true
        if ($Installation.Pointer.release -cne $result.archive_sha256) {
            & (Join-Path $PSScriptRoot 'package-install.ps1') @installArgs | Out-Null
            if (-not $?) { throw 'Local native payload activation failed' }
        }
        $verifiedLauncher=Assert-LocalDeployPath (Join-Path $Installation.Engine "releases/$($result.archive_sha256)/vcp-launch.exe")
        if ((Get-FileHash -LiteralPath $verifiedLauncher).Hash.ToLowerInvariant() -cne $launcherHash) { throw 'Installed payload launcher differs from the staged native build' }
        Replace-LocalDeployFile $verifiedLauncher $Installation.Launcher
        $newEngine=Join-Path $Installation.Engine "releases/$($result.archive_sha256)/vcp.exe"
        $selection=Assert-LocalDeploySelection $Installation $newEngine
        if ($selection.data_directory -cne $oldSelection.data_directory) { throw 'Deployment changed the resolved data directory' }
        if ((Invoke-LocalDeployExecutable $Installation.Launcher @('--version')) -cne "vcp $Version" -or
            (Get-FileHash -LiteralPath $Installation.Launcher).Hash.ToLowerInvariant() -cne $launcherHash) { throw 'Installed native version or launcher bytes differ' }
        $record=[ordered]@{schema='vcp-local-deploy/1';status='deployed-for-local-testing';version=$Version;version_incremented=$false;
            install_root=$Installation.App;executable=$newEngine;package_sha256=$result.archive_sha256;
            previous_launcher=$backupLauncher;previous_pointer=$backupPointer;completed_utc=[DateTime]::UtcNow.ToString('o');
            limitations=@('Development deployment of current checkout; no installer, VSIX, signing, release qualification or publication.')}
        $record | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'deploy-result.json') -Encoding utf8NoBOM
        return $record
    } catch {
        $failure=$_
        if ($changed) {
            try {
                $pointerPath=Join-Path $Installation.Engine 'active.json'
                if ((Get-FileHash -LiteralPath $backupPointer).Hash -cne (Get-FileHash -LiteralPath $pointerPath).Hash) {
                    Replace-LocalDeployFile $backupPointer $pointerPath
                }
                if ((Get-FileHash -LiteralPath $backupLauncher).Hash -cne (Get-FileHash -LiteralPath $Installation.Launcher).Hash) {
                    Replace-LocalDeployFile $backupLauncher $Installation.Launcher
                }
                $null=Assert-LocalDeploySelection $Installation (Join-Path $Installation.Engine "releases/$($Installation.Pointer.release)/vcp.exe")
            } catch { throw "Deployment failed and automatic restoration failed. Preserve backups in $EvidenceRoot. Restoration error: $($_.Exception.Message); deployment error: $($failure.Exception.Message)" }
        }
        throw $failure
    } finally {
        foreach ($mutex in @($installLock,$setupLock)) { if ($mutex) { $mutex.ReleaseMutex(); $mutex.Dispose() } }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    if (-not $IsWindows) { throw 'Local native deployment requires Windows and PowerShell 7' }
    $repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $installation=Get-LocalDeployInstallation $InstallRoot
    if ($UseLatestBuild) {
        $completed=Get-LocalDeployCompletedBuild $BuildRoot
        $version=$completed.Version
        $operation='Deploy the latest completed local native build without rebuilding or running the installer'
        Write-Host "Local deployment: completed VCP $version -> $($installation.App)"
    } else {
        if ($PSBoundParameters.ContainsKey('BuildRoot')) { throw '-BuildRoot requires -UseLatestBuild' }
        $channel=Read-LocalDeployJson (Join-Path $repository 'release/internal-beta.json')
        if ($channel.native_version -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Invalid current native version' }
        $version=$channel.native_version
        $operation='Rebuild current source and deploy native files for local testing, keeping the version unchanged'
        Write-Host "Local rebuild: VCP $version, $Jobs Cargo jobs -> $($installation.App)"
    }
    if (-not $PSCmdlet.ShouldProcess($installation.App,$operation)) { return }
    $artifacts=Join-Path $repository 'artifacts'
    if (-not (Test-Path -LiteralPath $artifacts)) { New-Item -ItemType Directory -Path $artifacts | Out-Null }
    $null=Assert-LocalDeployPath $artifacts -Directory
    $lockPath=Join-Path $artifacts 'local-setup-build.lock'
    if (Test-Path -LiteralPath $lockPath) { $null=Assert-LocalDeployPath $lockPath }
    try { $buildLock=[IO.File]::Open($lockPath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None) }
    catch [IO.IOException] { throw 'Another local setup or deployment build is using this checkout; wait for it to finish' }
    try {
        $out=Join-Path $artifacts ('local-deploy/' + [guid]::NewGuid().ToString('N'))
        $parent=Split-Path -Parent $out
        if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Path $parent | Out-Null }
        $null=Assert-LocalDeployPath $parent -Directory
        New-Item -ItemType Directory -Path $out | Out-Null
        $node=(Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        if ($UseLatestBuild) {
            $resultPath=$completed.PackageResult
            $receiptPath=$completed.ReceiptPath
            $receipt=Read-LocalDeployJson $receiptPath
        } else {
        $buildOutput=@(Invoke-LocalDeployBuild (Join-Path $PSScriptRoot 'build-production.ps1') (Join-Path $out 'build') $Jobs)
        $receiptPath=[string]$buildOutput[-1]
        $receipt=Read-LocalDeployJson $receiptPath
        if ($receipt.schema -cne 'vcp-local-build/1' -or $receipt.exit_code -ne 0 -or $receipt.release -or
            $receipt.qualification_build -ne $false -or $receipt.source_stable -ne $true -or $receipt.toolchain_stable -ne $true) { throw 'Successful stable local production build required' }
        & $node (Join-Path $PSScriptRoot 'installer/local-candidate.cjs') verify $repository $receiptPath $receipt.launcher | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Current-source native build verification failed' }
        $packageOutput=@(& (Join-Path $PSScriptRoot 'package.ps1') -Executable $receipt.executable -BuildReceipt $receiptPath -OutputRoot (Join-Path $out 'native'))
        $resultPath=[string]$packageOutput[-1]
        }
        $payloadRoot=Join-Path (Split-Path -Parent $resultPath) 'package'
        & $node -e 'const path=require("node:path"),p=require(process.argv[1]),i=require(process.argv[2]),root=process.argv[3],receipt=p.json(process.argv[4]),manifest=p.json(path.join(root,"manifest.json")),result=p.json(process.argv[5]);if(JSON.stringify(result.manifest)!==JSON.stringify(manifest))throw Error("Native result and payload manifest differ");i.verifyManifest(root,manifest);p.verifyPayloadSources(root,receipt);' (Join-Path $PSScriptRoot 'release/provenance.cjs') (Join-Path $PSScriptRoot 'package-inventory.cjs') $payloadRoot $receiptPath $resultPath
        if ($LASTEXITCODE -ne 0) { throw 'Staged native payload differs from current build inputs' }
        # Catch source edits during staging before changing any installed files.
        if (-not $UseLatestBuild) {
            & $node (Join-Path $PSScriptRoot 'installer/local-candidate.cjs') verify $repository $receiptPath $receipt.launcher | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'Source changed while staging local native files; rebuild before deployment' }
        }
        $deployed=Invoke-LocalDeployPayload $installation $resultPath $out $version
        Write-Host "Deployed VCP $($deployed.version) for local testing. Evidence: $(Join-Path $out 'deploy-result.json')"
    } finally { $buildLock.Dispose() }
}
