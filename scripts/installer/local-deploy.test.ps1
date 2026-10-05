# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Local deployment tests require Windows' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$scriptPath = Join-Path $repository 'scripts/build-local-deploy.ps1'
. $scriptPath
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $temporaryBase ('local-deploy-test-' + [guid]::NewGuid().ToString('N'))
$savedFlags = [Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')
$savedFault = [Environment]::GetEnvironmentVariable('VCP_PACKAGE_INSTALL_FAULT','Process')
$sourceVersion = (Get-FileHash (Join-Path $repository 'release/internal-beta.json')).Hash
$checks = 0
function Check([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
function Reject([scriptblock]$Action,[string]$Expected) {
    $reason = $null
    try { & $Action | Out-Null } catch { $reason = $_.Exception.Message }
    Check ($reason -and $reason -match $Expected) "Expected rejection '$Expected'; actual: $reason"
}
function Write-Json([string]$Path,$Value) { $Value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM }
function Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path).Hash }
function New-Payload([string]$Name) {
    $root = Join-Path $temporary $Name
    $package = Join-Path $root 'package'
    New-Item -ItemType Directory -Path (Join-Path $package 'runtime') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $package 'vcp.exe'),"engine-$Name")
    [IO.File]::WriteAllText((Join-Path $package 'vcp-launch.exe'),"launcher-$Name")
    [IO.File]::WriteAllText((Join-Path $package 'runtime/fixture.dll'),"runtime-$Name")
    Write-Json (Join-Path $package 'build-receipt.json') @{schema='vcp-local-build/1';launcher_sha256=(Hash (Join-Path $package 'vcp-launch.exe')).ToLowerInvariant()}
    $files = @(Get-ChildItem -LiteralPath $package -Recurse -File | ForEach-Object {
        @{path=$_.FullName.Substring($package.Length+1).Replace('\','/');bytes=$_.Length;sha256=(Hash $_.FullName).ToLowerInvariant()}
    })
    $manifest = @{schema='vcp-distribution-manifest/1';files=$files;model_provisioning=@{bundled=$false};
        compatibility=@{canonical='vcp-store/1+replay-base/2';config='vcp-cli-profile/1';index='derived-index-rebuild-required'}}
    Write-Json (Join-Path $package 'manifest.json') $manifest
    $archive = Join-Path $root 'vcp-windows-unsigned.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($package,$archive)
    $result = Join-Path $root 'result.json'
    Write-Json $result @{schema='vcp-distribution-result/1';status='candidate';package='vcp-windows-unsigned.zip';archive_sha256=(Hash $archive).ToLowerInvariant();manifest=$manifest}
    return $result
}
# The archive, manifests and installation lifecycle are real; only native
# process outputs are simulated, so fixtures never execute an installed app.
function Invoke-LocalDeployExecutable([string]$File,[string[]]$Arguments) {
    if ($Arguments[0] -ceq '--launcher-version') { return 'vcp-launch 0.2.9' }
    if ($Arguments[0] -ceq '--version') {
        if ($script:failVersion) { $script:failVersion=$false; return 'vcp wrong-version' }
        return 'vcp 0.2.9'
    }
    if ($Arguments[0] -ceq '--resolve-installation') {
        $engine = Join-Path (Split-Path -Parent $File) 'engine'
        $pointer = Get-Content -LiteralPath (Join-Path $engine 'active.json') -Raw | ConvertFrom-Json
        return (@{schema='vcp-installed-engine/1';executable=(Join-Path $engine "releases/$($pointer.release)/vcp.exe");data_directory=$pointer.data_root} | ConvertTo-Json)
    }
    throw 'Unexpected native fixture command'
}
try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    [Environment]::SetEnvironmentVariable('VCP_PACKAGE_INSTALL_FAULT',[NullString]::Value,'Process')
    $oldResult = New-Payload 'old'; $newResult = New-Payload 'new'
    $app = Join-Path $temporary 'Installed App β'; $engine = Join-Path $app 'engine'; $data = Join-Path $temporary 'Protected Data'
    New-Item -ItemType Directory -Path $app,$data | Out-Null
    [IO.File]::WriteAllText((Join-Path $data 'history.sentinel'),'preserve-history')
    $oldArchive = Join-Path (Split-Path -Parent $oldResult) 'vcp-windows-unsigned.zip'
    & (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $oldArchive -InstallRoot $engine -DataRoot $data | Out-Null
    if (-not $?) { throw 'Fixture install failed' }
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $oldResult) 'package/vcp-launch.exe') -Destination (Join-Path $app 'vcp.exe')
    $installation = Get-LocalDeployInstallation $app
    $pointerPath = Join-Path $engine 'active.json'; $launcherPath = Join-Path $app 'vcp.exe'
    $oldPointerHash = Hash $pointerPath; $oldLauncherHash = Hash $launcherPath
    $historyHash = Hash (Join-Path $data 'history.sentinel')

    $failedEvidence = Join-Path $temporary 'failed-deploy'; New-Item -ItemType Directory -Path $failedEvidence | Out-Null
    $script:failVersion = $true
    Reject { Invoke-LocalDeployPayload $installation $newResult $failedEvidence '0.2.9' } 'Installed native version or launcher bytes differ'
    Check ((Hash $pointerPath) -ceq $oldPointerHash -and (Hash $launcherPath) -ceq $oldLauncherHash) 'Late verification failure did not restore both active pointer and launcher'
    Check (-not (Test-Path (Join-Path $failedEvidence 'deploy-result.json'))) 'Failed deployment produced a success record'

    $tamperEvidence = Join-Path $temporary 'tampered-launcher'; New-Item -ItemType Directory -Path $tamperEvidence | Out-Null
    $stagedLauncher = Join-Path (Split-Path -Parent $newResult) 'package/vcp-launch.exe'
    $stagedBytes = [IO.File]::ReadAllBytes($stagedLauncher)
    try {
        [IO.File]::AppendAllText($stagedLauncher,'tampered')
        Reject { Invoke-LocalDeployPayload $installation $newResult $tamperEvidence '0.2.9' } 'Staged launcher differs'
        Check ((Hash $pointerPath) -ceq $oldPointerHash -and (Hash $launcherPath) -ceq $oldLauncherHash) 'Staged launcher tampering changed installation'
    } finally { [IO.File]::WriteAllBytes($stagedLauncher,$stagedBytes) }
    $newArchive = Join-Path (Split-Path -Parent $newResult) 'vcp-windows-unsigned.zip'
    $archiveBytes = [IO.File]::ReadAllBytes($newArchive)
    try {
        [IO.File]::AppendAllText($newArchive,'tampered')
        Reject { Invoke-LocalDeployPayload $installation $newResult $tamperEvidence '0.2.9' } 'Native payload archive changed'
        Check ((Hash $pointerPath) -ceq $oldPointerHash -and (Hash $launcherPath) -ceq $oldLauncherHash) 'Archive tampering changed installation'
    } finally { [IO.File]::WriteAllBytes($newArchive,$archiveBytes) }

    $faultEvidence = Join-Path $temporary 'before-activation'; New-Item -ItemType Directory -Path $faultEvidence | Out-Null
    [Environment]::SetEnvironmentVariable('VCP_PACKAGE_INSTALL_FAULT','before-activation','Process')
    try {
        Reject { Invoke-LocalDeployPayload $installation $newResult $faultEvidence '0.2.9' } 'Deterministic fault before active pointer replacement'
        Check ((Hash $pointerPath) -ceq $oldPointerHash -and (Hash $launcherPath) -ceq $oldLauncherHash) 'Activation failure changed previous pointer or launcher'
    } finally { [Environment]::SetEnvironmentVariable('VCP_PACKAGE_INSTALL_FAULT',[NullString]::Value,'Process') }

    $busyEvidence = Join-Path $temporary 'busy-launcher'; New-Item -ItemType Directory -Path $busyEvidence | Out-Null
    $busy = [IO.File]::Open($launcherPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        Reject { Invoke-LocalDeployPayload $installation $newResult $busyEvidence '0.2.9' } 'being used by another process|access.*denied'
        Check ((Hash $pointerPath) -ceq $oldPointerHash -and (Hash $launcherPath) -ceq $oldLauncherHash) 'Busy launcher left the new payload activated'
    } finally { $busy.Dispose() }

    $evidence = Join-Path $temporary 'success'; New-Item -ItemType Directory -Path $evidence | Out-Null
    $result = Invoke-LocalDeployPayload $installation $newResult $evidence '0.2.9'
    Check ($result.version -ceq '0.2.9' -and $result.version_incremented -ceq $false) 'Local deployment changed its version'
    Check ((Hash $launcherPath) -ceq (Hash (Join-Path (Split-Path -Parent $newResult) 'package/vcp-launch.exe'))) 'Successful deployment did not replace launcher'
    Check ([IO.File]::ReadAllText((Join-Path (Split-Path -Parent $result.executable) 'runtime/fixture.dll')) -ceq 'runtime-new') 'Accompanying native runtime file was not deployed'
    Check ((Hash (Join-Path $data 'history.sentinel')) -ceq $historyHash) 'Protected user data changed'
    Check ((Read-LocalDeployJson (Join-Path $evidence 'deploy-result.json')).status -ceq 'deployed-for-local-testing') 'Deployment evidence missing'

    $samePointer = Hash $pointerPath
    $repeatEvidence = Join-Path $temporary 'repeated-deploy'; New-Item -ItemType Directory -Path $repeatEvidence | Out-Null
    $null = Invoke-LocalDeployPayload (Get-LocalDeployInstallation $app) $newResult $repeatEvidence '0.2.9'
    Check ((Hash $pointerPath) -ceq $samePointer) 'Repeated identical payload rewrote pointer or previous release'

    # Reuse a completed setup's unchanged native ZIP without a compiler or setup
    # invocation. The setup executable is inert selection-fixture bytes.
    $completedRoot=Join-Path $temporary 'completed-builds'
    $completed=Join-Path $completedRoot '0.2.9/run'
    New-Item -ItemType Directory -Path $completed -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $newResult) 'package') -Destination (Join-Path $completed 'package') -Recurse
    $receiptPath=Join-Path $completed 'package/build-receipt.json'
    Write-Json $receiptPath @{schema='vcp-local-build/1';exit_code=0;cargo_exit_code=0;source_stable=$true;toolchain_stable=$true;qualification_build=$false;launcher_sha256=(Hash (Join-Path $completed 'package/vcp-launch.exe')).ToLowerInvariant()}
    $candidate='a' * 64
    $manifest=@{schema='vcp-distribution-manifest/1';artifact='unsigned-local-candidate';signing=@{status='unsigned'};
        local_candidate=@{schema='vcp-local-candidate/1';native_version='0.2.9';signing='unsigned';candidate_id=$candidate};
        model_provisioning=@{bundled=$false};compatibility=@{canonical='vcp-store/1+replay-base/2';config='vcp-cli-profile/1';index='derived-index-rebuild-required'};
        files=@(Get-ChildItem -LiteralPath (Join-Path $completed 'package') -Recurse -File | Where-Object Name -ne 'manifest.json' | ForEach-Object {
            @{path=$_.FullName.Substring((Join-Path $completed 'package').Length+1).Replace('\','/');bytes=$_.Length;sha256=(Hash $_.FullName).ToLowerInvariant()}
        })}
    Write-Json (Join-Path $completed 'package/manifest.json') $manifest
    $archiveName='vcp-0.2.9-windows-x64-unsigned-local.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory((Join-Path $completed 'package'),(Join-Path $completed $archiveName))
    $nativePath=Join-Path $completed 'native-result.json'
    $archiveHash=(Hash (Join-Path $completed $archiveName)).ToLowerInvariant()
    Write-Json $nativePath @{schema='vcp-distribution-result/1';status='candidate';package=$archiveName;archive_sha256=$archiveHash;manifest=$manifest}
    $setupName='vcp-0.2.9-windows-x64-unsigned-setup.exe'
    [IO.File]::WriteAllText((Join-Path $completed $setupName),'inert setup fixture')
    $setupPath=Join-Path $completed 'setup-result.json'
    $setup=@{schema='vcp-setup-result/1';status='unsigned-local-candidate';local_candidate=$manifest.local_candidate;candidate_id=$candidate;
        archive=@{file=$setupName;sha256=(Hash (Join-Path $completed $setupName)).ToLowerInvariant()};native_result='native-result.json';
        native_archive_sha256=$archiveHash;build_receipt_sha256=(Hash $receiptPath).ToLowerInvariant();launcher_sha256=(Hash (Join-Path $completed 'package/vcp-launch.exe')).ToLowerInvariant()}
    Write-Json $setupPath $setup
    New-Item -ItemType Directory -Path (Join-Path $completedRoot '0.2.10/interrupted') -Force | Out-Null
    $selection=Get-LocalDeployCompletedBuild $completedRoot
    Check ($selection.Version -ceq '0.2.9' -and $selection.PackageResult -ceq $nativePath) 'Completed native build selection used an incomplete build'
    $reuseEvidence=Join-Path $temporary 'reused-deploy'; New-Item -ItemType Directory -Path $reuseEvidence | Out-Null
    $reused=Invoke-LocalDeployPayload $installation $selection.PackageResult $reuseEvidence $selection.Version
    Check ($reused.package_sha256 -ceq $archiveHash) 'Completed archive was rebuilt or changed during deployment'
    Check ((Hash (Join-Path $data 'history.sentinel')) -ceq $historyHash) 'Completed-build deployment changed user data'
    $setupBytes=[IO.File]::ReadAllBytes($setupPath)
    try {
        $setup.native_archive_sha256='b' * 64; Write-Json $setupPath $setup
        Reject { Get-LocalDeployCompletedBuild $completedRoot } 'payload identities differ'
    } finally { [IO.File]::WriteAllBytes($setupPath,$setupBytes) }
    $receiptBytes=[IO.File]::ReadAllBytes($receiptPath)
    try {
        [IO.File]::AppendAllText($receiptPath,' ')
        Reject { Get-LocalDeployCompletedBuild $completedRoot } 'build receipt differs'
    } finally { [IO.File]::WriteAllBytes($receiptPath,$receiptBytes) }
    foreach ($path in @('relative/path','D:relative','\\server\share\app',(Join-Path $app '../Installed App β'),'D:\')) {
        Reject { Assert-LocalDeployPath $path -Directory } 'absolute local drive|drive root'
    }
    $junction = Join-Path $temporary 'redirected-app'
    New-Item -ItemType Junction -Path $junction -Target $app | Out-Null
    try { Reject { Get-LocalDeployInstallation $junction } 'Redirected deployment path' }
    finally { Remove-Item -LiteralPath $junction -Force }
    $ownerPath = Join-Path $engine '.vcp-install-owned.json'; $ownerBytes = [IO.File]::ReadAllBytes($ownerPath)
    try {
        $owner = Read-LocalDeployJson $ownerPath; $owner.install_root = Join-Path $temporary 'wrong-owner'
        Write-Json $ownerPath $owner
        Reject { Get-LocalDeployInstallation $app } 'ownership or active release'
    } finally { [IO.File]::WriteAllBytes($ownerPath,$ownerBytes) }
    $pointerBytes = [IO.File]::ReadAllBytes($pointerPath)
    try {
        $owner = Read-LocalDeployJson $ownerPath; $pointer = Read-LocalDeployJson $pointerPath
        $owner.data_root = Join-Path $app 'unsafe-data'; $pointer.data_root = $owner.data_root
        Write-Json $ownerPath $owner; Write-Json $pointerPath $pointer
        Reject { Get-LocalDeployInstallation $app } 'protected data must be disjoint'
    } finally { [IO.File]::WriteAllBytes($ownerPath,$ownerBytes); [IO.File]::WriteAllBytes($pointerPath,$pointerBytes) }
    function Get-ItemProperty { param([string]$LiteralPath,$ErrorAction) return @{InstallLocation=$(if($LiteralPath.StartsWith('HKCU')){$app}else{Join-Path $temporary 'different-app'})} }
    try { Reject { Get-LocalDeployInstallation '' } 'Multiple registered VCP installations' }
    finally { Remove-Item Function:Get-ItemProperty }

    $fixture = Join-Path $temporary 'checkout'
    New-Item -ItemType Directory -Path (Join-Path $fixture 'scripts'),(Join-Path $fixture 'release'),(Join-Path $fixture 'artifacts') | Out-Null
    $fixtureScript = Join-Path $fixture 'scripts/build-local-deploy.ps1'
    Copy-Item -LiteralPath $scriptPath -Destination $fixtureScript
    Copy-Item -LiteralPath (Join-Path $repository 'scripts/install-local.ps1') -Destination (Join-Path $fixture 'scripts/install-local.ps1')
    Write-Json (Join-Path $fixture 'release/internal-beta.json') @{native_version='0.2.9'}
    [IO.File]::WriteAllText((Join-Path $fixture 'scripts/build-production.ps1'),"param([string]`$OutputRoot,[int]`$Jobs)`nthrow 'fixture-build-failed'")
    $beforePointer = Hash $pointerPath; $beforeLauncher = Hash $launcherPath
    $whatIf = & pwsh -NoProfile -File $fixtureScript -InstallRoot $app -WhatIf 2>&1 | Out-String
    Check ($LASTEXITCODE -eq 0 -and $whatIf -match 'What if:' -and -not @(Get-ChildItem (Join-Path $fixture 'artifacts') -Force).Count) 'WhatIf built or mutated artifacts'
    Write-Json (Join-Path $fixture 'release/internal-beta.json') @{native_version='0.2.99'}
    $reusePreview=& pwsh -NoProfile -File $fixtureScript -InstallRoot $app -UseLatestBuild -BuildRoot $completedRoot -WhatIf 2>&1 | Out-String
    Check ($LASTEXITCODE -eq 0 -and $reusePreview -match 'completed VCP 0.2.9' -and $reusePreview -match 'without rebuilding' -and -not @(Get-ChildItem (Join-Path $fixture 'artifacts') -Force).Count) 'Reuse preview built files or selected the current source instead of the completed artifact'
    $buildFailure = & pwsh -NoProfile -File $fixtureScript -InstallRoot $app 2>&1 | Out-String
    Check ($LASTEXITCODE -ne 0 -and $buildFailure -match 'fixture-build-failed') 'Build failure was not propagated'
    Check ((Hash $pointerPath) -ceq $beforePointer -and (Hash $launcherPath) -ceq $beforeLauncher) 'Build failure or WhatIf changed installation'
    $buildLock = [IO.File]::Open((Join-Path $fixture 'artifacts/local-setup-build.lock'),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try {
        $diagnostic = & pwsh -NoProfile -File $fixtureScript -InstallRoot $app 2>&1 | Out-String
        Check ($LASTEXITCODE -ne 0 -and $diagnostic -match 'Another local setup or deployment build') 'Concurrent checkout build was allowed'
    } finally { $buildLock.Dispose() }

    $mutexName = 'Local\VCP.LocalDeploy.Test.' + [guid]::NewGuid().ToString('N')
    $mutex = Enter-LocalDeployMutex $mutexName
    try {
        $lockCheck = Join-Path $temporary 'lock-check.ps1'
        [IO.File]::WriteAllText($lockCheck,". '$($scriptPath.Replace("'","''"))'`n`$held=Enter-LocalDeployMutex '$mutexName'`n`$held.ReleaseMutex(); `$held.Dispose()")
        $diagnostic = & pwsh -NoProfile -File $lockCheck 2>&1 | Out-String
        Check ($LASTEXITCODE -ne 0 -and $diagnostic -match 'Another VCP installation operation') 'Concurrent named installation operation was allowed'
    } finally { $mutex.ReleaseMutex(); $mutex.Dispose() }

    $recipe = @('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31
    $stubBuild = Join-Path $temporary 'environment-build.ps1'
    [IO.File]::WriteAllText($stubBuild,@'
param([string]$OutputRoot,[int]$Jobs)
if ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')) { throw 'Unexpected build override: CARGO_ENCODED_RUSTFLAGS' }
if ($Jobs -ne 16) { throw 'Wrong job count' }
if ($OutputRoot -eq 'fail') { throw 'fixture-build-failed' }
'fixture-receipt'
'@)
    foreach ($flags in @($null,$recipe)) {
        [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$(if($null -eq $flags){[NullString]::Value}else{$flags}),'Process')
        Check ((Invoke-LocalDeployBuild $stubBuild 'success' 16) -ceq 'fixture-receipt') 'Valid local build environment failed'
        Reject { Invoke-LocalDeployBuild $stubBuild 'fail' 16 } 'fixture-build-failed'
        Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq $flags) 'Caller flags were not restored'
    }
    [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','custom-rustflags','Process')
    Reject { Invoke-LocalDeployBuild $stubBuild 'success' 16 } 'Unexpected build override'
    Check ([Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process') -ceq 'custom-rustflags') 'Custom caller flags were erased'
    Check ((Hash (Join-Path $repository 'release/internal-beta.json')) -ceq $sourceVersion) 'Source version metadata changed'
    $tokens=$null; $errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$tokens,[ref]$errors)
    Check ($errors.Count -eq 0) 'Deployment script has PowerShell parse errors'
    $commands = ($ast.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst]},$true).Extent.Text -join "`n")
    Check ($commands -notmatch 'build-local-setup|local-version|npm|vsix|signing-file|compiler-installer') 'Deployment invoked installer, extension, signing or versioning workflow'
    Write-Output "PASS $checks local native deployment assertions (synthetic installations; real archive activation and rollback)"
} finally {
    foreach ($pair in @(@('CARGO_ENCODED_RUSTFLAGS',$savedFlags),@('VCP_PACKAGE_INSTALL_FAULT',$savedFault))) {
        [Environment]::SetEnvironmentVariable($pair[0],$(if($null -eq $pair[1]){[NullString]::Value}else{$pair[1]}),'Process')
    }
    $resolved = [IO.Path]::GetFullPath($temporary)
    if (-not $resolved.StartsWith([IO.Path]::GetFullPath($temporaryBase).TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'local-deploy-test-*') { throw 'Fixture cleanup escaped artifact root' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
