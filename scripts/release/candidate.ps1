# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$ReviewedCommit,
    [Parameter(Mandatory)][string]$OutputRoot,
    [ValidateRange(1,8)][int]$Jobs = 2
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows candidate builder required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $PSScriptRoot 'editor-layout.ps1')
$tools=Get-Content -LiteralPath (Join-Path $repository 'release/candidate-tools.json') -Raw | ConvertFrom-Json
$EditorArchiveSha256=$tools.editor.sha256
if ($tools.schema -cne 'vcp-candidate-tools/1' -or $EditorArchiveSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Pinned candidate tools required' }
$out = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $out) { throw 'New candidate output directory required' }
New-Item -ItemType Directory -Path (Join-Path $out 'logs') -Force | Out-Null
$runPath = Join-Path $out 'run.json'
$run = [ordered]@{
    schema='vcp-candidate-run/1'; status='running'; reviewed_commit=$ReviewedCommit
    started_at=[DateTime]::UtcNow.ToString('o'); stages=@(); receipts=@{}
    environment=@{os=[Runtime.InteropServices.RuntimeInformation]::OSDescription; architecture=[Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); node='not observed'; powershell=$PSVersionTable.PSVersion.ToString(); runner_image=$env:ImageOS; runner_image_version=$env:ImageVersion; editor_archive_sha256=$EditorArchiveSha256; editor_version='1.138.0'; host='Windows build image with development tools; not clean standard-user qualification'}
}
function Save-Run { $run | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $runPath -Encoding utf8NoBOM }
function Get-CandidateDiskEvidence {
    $volumes=@(); $errors=@()
    foreach ($drive in [IO.DriveInfo]::GetDrives()) {
        if ($drive.DriveType -ne [IO.DriveType]::Fixed) { continue }
        try {
            if (-not $drive.IsReady) { throw 'Volume is not ready' }
            $volumes+=@{root=$drive.Name;capacity_bytes=$drive.TotalSize;available_free_bytes=$drive.AvailableFreeSpace;total_free_bytes=$drive.TotalFreeSpace}
        } catch { $errors+=@{root=$drive.Name;reason=$_.Exception.Message} }
    }
    return @{captured_at=[DateTime]::UtcNow.ToString('o');volumes=$volumes;errors=$errors}
}
function Assert-CandidateOrdinaryPath([string]$Path,[bool]$Directory) {
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw 'Cleanup requires an absolute ordinary path' }
    $full=[IO.Path]::GetFullPath($Path)
    if ($full.TrimEnd('\') -ine $Path.TrimEnd('\')) { throw 'Cleanup refuses noncanonical path components' }
    $item=Get-Item -LiteralPath $full -Force -ErrorAction Stop
    if ([bool]$item.PSIsContainer -ne $Directory) { throw 'Cleanup path has the wrong file type' }
    for ($current=$item; $current; $current=if ($current -is [IO.DirectoryInfo]) {$current.Parent} else {$current.Directory}) {
        if ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cleanup refuses redirected paths or ancestors' }
    }
    return $item.FullName
}
function Set-CandidateTemporaryDirectory([string]$Node,$Evidence) {
    $Evidence.inherited_temp=$env:TEMP; $Evidence.inherited_tmp=$env:TMP
    $Evidence.requested='not observed'; $Evidence.selected='not observed'
    # Hosted Windows images may spell TEMP with an 8.3 alias or a junction.
    # Select its physical directory before fixtures are created; their ordinary
    # path guards still reject redirected paths supplied to the tested APIs.
    $resolved=& $Node '-e' "const fs=require('node:fs'),os=require('node:os');const requested=os.tmpdir();console.log(JSON.stringify({requested,selected:fs.realpathSync.native(requested)}))"
    if ($LASTEXITCODE -ne 0 -or $resolved -isnot [string]) { throw 'Candidate temporary directory resolution failed' }
    $selection=$resolved | ConvertFrom-Json
    $Evidence.requested=$selection.requested
    $selected=Assert-CandidateOrdinaryPath $selection.selected $true
    $env:TEMP=$selected; $env:TMP=$selected
    $Evidence.selected=$selected
    return $selected
}
function Remove-CandidateProductionTarget([string]$CandidateRoot,$Run) {
    # Only this invocation's fresh, successfully paired production build is
    # disposable. Copied programs, symbols, receipts, logs and caches are not.
    foreach ($id in @('production-build','native-package','setup-package','vsix-package')) {
        $stage=@($Run.stages | Where-Object id -CEQ $id)
        if ($stage.Count -ne 1 -or $stage[0].status -cne 'pass') { throw 'Cleanup requires successful production packaging' }
    }
    $root=Assert-CandidateOrdinaryPath $CandidateRoot $true
    $pairFile=Assert-CandidateOrdinaryPath (Join-Path $root 'pair.json') $false
    $pairStage=@($Run.stages | Where-Object id -CEQ 'pair')
    if ($pairStage.Count -ne 1 -or $pairStage[0].status -cnotin @('running','pass') -or
        $pairStage[0].verified_pair_sha256 -cne (Get-FileHash -LiteralPath $pairFile).Hash.ToLowerInvariant()) { throw 'Cleanup requires the successful exact pair validation' }
    $receiptFile=Assert-CandidateOrdinaryPath $Run.receipts.build $false
    $buildRoot=Split-Path -Parent $receiptFile
    $id=Split-Path -Leaf $buildRoot
    if ($id -cnotmatch '^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$' -or
        $receiptFile -ine (Join-Path $root "build/$id/build-receipt.json")) { throw 'Cleanup receipt is outside the exact owned build directory' }
    $receipt=Get-Content -LiteralPath $receiptFile -Raw | ConvertFrom-Json
    $pair=Get-Content -LiteralPath $pairFile -Raw | ConvertFrom-Json
    if ($receipt.schema -cne 'vcp-local-build/1' -or $receipt.exit_code -ne 0 -or $receipt.cargo_exit_code -ne 0 -or
        $receipt.qualification_build -ne $false -or $receipt.profile -cne 'release' -or
        $pair.schema -cne 'vcp-release-pair/1' -or $pair.release.candidate_id -cnotmatch '^[a-f0-9]{64}$' -or
        $pair.release.candidate_id -cne $receipt.release.candidate_id) { throw 'Cleanup requires the paired production build receipt' }
    $target=Join-Path $buildRoot 'cargo-target'
    if (@($receipt.command).Count -ne 20 -or $receipt.command[15] -cne '--target-dir' -or
        -not [IO.Path]::IsPathFullyQualified($receipt.command[16]) -or $receipt.command[16] -ine $target) { throw 'Cleanup refuses an unexpected Cargo target' }
    foreach ($row in @(
        @{name='vcp.exe';path=$receipt.executable;sha256=$receipt.executable_sha256;artifact=$receipt.compiler_artifact},
        @{name='vcp-launch.exe';path=$receipt.launcher;sha256=$receipt.launcher_sha256;artifact=$receipt.launcher_compiler_artifact}
    )) {
        $copied=Assert-CandidateOrdinaryPath $row.path $false
        if ($copied -ine (Join-Path $buildRoot $row.name) -or
            (Get-FileHash -LiteralPath $copied).Hash.ToLowerInvariant() -cne $row.sha256 -or
            $row.artifact.executable -ine (Join-Path $target "x86_64-pc-windows-msvc/release/$($row.name)")) { throw 'Cleanup requires preserved copied production executables' }
    }
    if ($receipt.symbols_sha256) {
        $symbols=Assert-CandidateOrdinaryPath (Join-Path $buildRoot 'vcp.pdb') $false
        if ((Get-FileHash -LiteralPath $symbols).Hash.ToLowerInvariant() -cne $receipt.symbols_sha256) { throw 'Cleanup requires preserved production symbols' }
    }
    $target=Assert-CandidateOrdinaryPath $target $true
    $pending=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $pending.Push([IO.DirectoryInfo]::new($target))
    [long]$bytes=0; [long]$files=0
    while ($pending.Count) {
        foreach ($item in $pending.Pop().GetFileSystemInfos()) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cleanup refuses a redirected entry in the Cargo target' }
            if ($item -is [IO.DirectoryInfo]) { $pending.Push($item) } else { $bytes+=$item.Length; $files++ }
        }
    }
    $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($target))
    $before=$drive.TotalFreeSpace
    # Repeat the resolved target/ancestor check immediately before the only
    # recursive removal. The producer is finished; no compiler shares this tree.
    $checked=Assert-CandidateOrdinaryPath $target $true
    if ($checked -ine (Join-Path $root "build/$id/cargo-target")) { throw 'Cleanup target escaped its owned build directory' }
    Remove-Item -LiteralPath $checked -Recurse -Force -ErrorAction Stop
    if (Test-Path -LiteralPath $checked) { throw 'Production Cargo target removal incomplete' }
    $drive=[IO.DriveInfo]::new($drive.Name)
    $after=$drive.TotalFreeSpace
    return @{status='removed';path=$checked;removed_files=$files;removed_file_bytes=$bytes;
        volume=$drive.Name;volume_free_bytes_before=$before;volume_free_bytes_after=$after;
        observed_recovered_bytes=($after-$before);ended_at=[DateTime]::UtcNow.ToString('o')}
}
function Stage([string]$Id,[string[]]$Command,[string]$Expected,[scriptblock]$Body) {
    $log = Join-Path $out "logs/$Id.log"
    $row = @{id=$Id;status='running';command=$Command;expected=$Expected;started_at=[DateTime]::UtcNow.ToString('o');log=$log;exit_code=$null;disk_before=(Get-CandidateDiskEvidence)}
    $run.stages += $row; Save-Run
    try {
        & $Body *> $log
        if ($Id -in @('native-boundaries','installed-editor')) {
            $text=Get-Content -LiteralPath $log -Raw
            $passed=0
            foreach ($match in [regex]::Matches($text,'test result: ok\. (\d+) passed')) { $passed += [int]$match.Groups[1].Value }
            $minimum=2
            if ($passed -lt $minimum) { throw 'Native command did not execute the required test cases' }
            $row.observed_passed=$passed
        }
        $row.status='pass'; $row.exit_code=0
    } catch {
        $row.status='fail'; $row.exit_code=1; $row.reason=$_.Exception.Message
        $_.Exception.Message | Add-Content -LiteralPath $log
        throw
    } finally { $row.ended_at=[DateTime]::UtcNow.ToString('o'); $row.disk_after=Get-CandidateDiskEvidence; Save-Run }
}
function Checked([string]$File,[string[]]$Arguments) {
    @{command=@($File)+$Arguments} | ConvertTo-Json -Compress -Depth 5 | Write-Output
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Command failed ($LASTEXITCODE): $File" }
}
function One-Result([string]$Root,[string]$Name) {
    $found = @(Get-ChildItem -LiteralPath $Root -Filter $Name -File -Recurse)
    if ($found.Count -ne 1) { throw "Expected one $Name under candidate stage" }
    return $found[0].FullName
}
function Download([string]$Uri,[string]$File,[string]$Digest) {
    Invoke-WebRequest -Uri $Uri -OutFile $File
    if ((Get-FileHash -LiteralPath $File).Hash.ToLowerInvariant() -cne $Digest) { throw 'Pinned download hash mismatch' }
}
Save-Run
if (Test-Path -LiteralPath (Join-Path $repository 'artifacts/beta-gate/delivery.json')) { $run.receipts.delivery=Join-Path $repository 'artifacts/beta-gate/delivery.json' }
$workspace = Join-Path $repository 'src/third_party/codex/codex-rs'
$channel = Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json
try {
    Stage 'source-gate' @('node','scripts/release/provenance.cjs','source',$repository,$ReviewedCommit) 'Clean exact reviewed source and channel versions.' {
        $script:pwsh = (Get-Command pwsh -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        $script:node = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        # Keep tool discovery inside the recorded stage. Hosted images can have
        # several installations; select the PATH winner, then resolve junctions.
        $script:node = (& $node '-p' "require('fs').realpathSync(process.execPath)").Trim()
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $node -PathType Leaf)) { throw 'Selected Node executable could not be resolved' }
        $env:PATH=(Split-Path -Parent $node)+';'+$env:PATH
        $nodeVersion=& $node --version
        if ($LASTEXITCODE -ne 0 -or $nodeVersion -isnot [string] -or $nodeVersion -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Selected Node version probe failed' }
        $run.environment.node=$nodeVersion
        if ($nodeVersion -cne ('v'+$tools.node)) { throw 'Candidate Node version differs from the reviewed tools pin' }
        $run.environment.temporary_directory=@{}
        $temporary=Set-CandidateTemporaryDirectory $node $run.environment.temporary_directory
        $script:private=Join-Path $temporary ('vcp-beta-private-' + [guid]::NewGuid())
        $run.environment.qualification_root=$private
        Checked $node @((Join-Path $PSScriptRoot 'provenance.cjs'),'source',$repository,$ReviewedCommit)
    }
    Stage 'provision' @('rustup','toolchain','install','1.95.0','1.98.0','--profile','minimal',';','cargo','+1.95.0','fetch','--locked','--target','x86_64-pc-windows-msvc') 'Explicit locked cache and pinned compiler/editor inputs; no provider or model acquisition.' {
        foreach ($version in @('1.95.0','1.98.0')) { Checked 'rustup' @('toolchain','install',$version,'--profile','minimal') }
        Push-Location $workspace
        try { Checked 'cargo' @('+1.95.0','fetch','--locked','--target','x86_64-pc-windows-msvc') } finally { Pop-Location }
        $script:compilerInstaller = Join-Path $out ('innosetup-' + $channel.installer.version + '.exe')
        Download $channel.installer.download_url $compilerInstaller $channel.installer.sha256
        $editorZip = Join-Path $out 'vscode.zip'
        if ($tools.editor.version -cne $channel.vscode_version) { throw 'Editor pin differs from supported channel' }
        Download $tools.editor.url $editorZip $EditorArchiveSha256
        $script:editor = Join-Path $out 'editor'
        Expand-Archive -LiteralPath $editorZip -DestinationPath $editor
        $editorLayout=Resolve-BetaEditor -Code (Join-Path $editor 'Code.exe')
        if ($editorLayout.version -cne $channel.vscode_version) { throw 'Pinned editor version mismatch' }
        $run.environment.editor_layout=$editorLayout
        $run.environment.editor_executable_sha256=$editorLayout.code_sha256
        $env:VCP_TEST_BETA_EDITOR_ARCHIVE=$editorZip
        $env:VCP_TEST_BETA_EDITOR_CODE=$editorLayout.code
    }
    Stage 'portable-contracts' @('node','--test','--test-name-pattern','^prepared official archive resolves actual versioned layout','src/tests/contracts/editor-layout.test.cjs',';','npm.cmd','test','--prefix','src/packages/sdk-ts',';','npm.cmd','test','--prefix','src/packages/vscode',';','pwsh','-File','scripts/test.ps1','-Suite','fast') 'Pinned editor bytes, SDK/editor and fast contracts pass on the current Windows source.' {
        # The general harness deliberately strips qualification input variables.
        # Execute this pinned-byte case directly while its explicit inputs exist.
        Checked $node @('--test','--test-name-pattern','^prepared official archive resolves actual versioned layout',(Join-Path $repository 'src/tests/contracts/editor-layout.test.cjs'))
        Checked 'npm.cmd' @('test','--prefix',(Join-Path $repository 'src/packages/sdk-ts'))
        Checked 'npm.cmd' @('test','--prefix',(Join-Path $repository 'src/packages/vscode'))
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/test.ps1'),'-Suite','fast','-OutputRoot',(Join-Path $out 'contracts'))
    }
    Stage 'production-build' @('pwsh','-File','scripts/build-production.ps1','-Release','-ReviewedCommit',$ReviewedCommit,'-Jobs',"$Jobs",'-OutputRoot',(Join-Path $out 'build')) 'Fresh offline Rust1.95 release build, no qualification features; source/cache/tool receipts stable.' {
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/build-production.ps1'),'-Release','-ReviewedCommit',$ReviewedCommit,'-Jobs',"$Jobs",'-OutputRoot',(Join-Path $out 'build'))
        $run.receipts.build=One-Result (Join-Path $out 'build') 'build-receipt.json'
        $script:build = Get-Content -LiteralPath $run.receipts.build -Raw | ConvertFrom-Json
    }
    Stage 'native-package' @('pwsh','-File','scripts/package.ps1','-Release','-ReviewedCommit',$ReviewedCommit,'-BuildReceipt',$run.receipts.build,'-Executable',$build.executable,'-OutputRoot',(Join-Path $out 'native')) 'Strict native ZIP with bound notices, assets and compatibility.' {
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/package.ps1'),'-Release','-ReviewedCommit',$ReviewedCommit,'-BuildReceipt',$run.receipts.build,'-Executable',$build.executable,'-OutputRoot',(Join-Path $out 'native'))
        $run.receipts.native=One-Result (Join-Path $out 'native') 'result.json'
    }
    Stage 'setup-package' @('pwsh','-File','scripts/build-setup.ps1','-ReviewedCommit',$ReviewedCommit,'-NativeResult',$run.receipts.native,'-BuildReceipt',$run.receipts.build,'-Launcher',$build.launcher,'-CompilerInstaller',$compilerInstaller,'-OutputRoot',(Join-Path $out 'setup')) 'Pinned compiler builds the setup bound to this native ZIP and launcher.' {
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/build-setup.ps1'),'-ReviewedCommit',$ReviewedCommit,'-NativeResult',$run.receipts.native,'-BuildReceipt',$run.receipts.build,'-Launcher',$build.launcher,'-CompilerInstaller',$compilerInstaller,'-OutputRoot',(Join-Path $out 'setup'))
        $run.receipts.setup=One-Result (Join-Path $out 'setup') 'setup-result.json'
    }
    $stagedEngine=Join-Path (Split-Path -Parent $run.receipts.native) 'package/vcp.exe'
    Stage 'vsix-package' @('node','src/packages/vscode/scripts/package.cjs','--reviewed-commit',$ReviewedCommit,'--build-receipt',$run.receipts.build,'--engine',$stagedEngine,'--engine-manifest',$run.receipts.native,'--output',(Join-Path $out 'vsix')) 'Actual strict beta VSIX with same source/native build and complete original build evidence.' {
        Checked $node @((Join-Path $repository 'src/packages/vscode/scripts/package.cjs'),'--reviewed-commit',$ReviewedCommit,'--build-receipt',$run.receipts.build,'--engine',$stagedEngine,'--engine-manifest',$run.receipts.native,'--output',(Join-Path $out 'vsix'))
        $run.receipts.vsix=Join-Path $out 'vsix/manifest.json'
    }
    Stage 'pair' @('node','scripts/release/pair.cjs',$run.receipts.native,$run.receipts.vsix,(Join-Path $out 'pair.json'),$run.receipts.setup) 'Independently hash all three final artifacts and require exact candidate identity.' {
        Checked $node @((Join-Path $PSScriptRoot 'pair.cjs'),$run.receipts.native,$run.receipts.vsix,(Join-Path $out 'pair.json'),$run.receipts.setup)
        $pairRow=@($run.stages | Where-Object id -CEQ 'pair')[0]
        $pairRow.verified_pair_sha256=(Get-FileHash -LiteralPath (Join-Path $out 'pair.json')).Hash.ToLowerInvariant()
        $run.production_target_cleanup=Remove-CandidateProductionTarget $out $run
        $run.production_target_cleanup | ConvertTo-Json -Depth 5
    }
    Stage 'native-boundaries' @('cargo','+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','local_execution_parity','--test','installed_launcher','--test','beta_launcher_console','--test','beta_editor_candidate','--','--test-threads=1') 'Separate qualification target: real CLI/client import parity and native launcher contract regressions.' {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
        foreach ($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin','Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')) { $env:PATH=(Join-Path $vsRoot $relative)+';'+$env:PATH }
        $env:VCP_TEST_NODE=$node; $env:VCP_TEST_GIT=(Get-Command git -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source; $env:CODEX_TEST_ENVIRONMENT='local'; $env:RUST_MIN_STACK='16777216'
        Push-Location $workspace
        try { Checked 'cargo' @('+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','local_execution_parity','--test','installed_launcher','--test','beta_launcher_console','--test','beta_editor_candidate','--','--test-threads=1') } finally { Pop-Location }
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/package-install.test.ps1'))
    }
    $consoleTest=One-Result (Join-Path $out 'qualification-target') 'beta_launcher_console-*.exe'
    Stage 'installed-native' @('pwsh','-File','scripts/release/candidate-smoke.ps1','-NativeResult',$run.receipts.native,'-SetupResult',$run.receipts.setup,'-OutputRoot',(Join-Path $private 'native'),'-ConsoleTestExecutable',$consoleTest) 'Install exact setup outside checkout, launch exact engine, verify both-store console cancellation, preserve data and uninstall; hosted-image observation only.' {
        Checked $pwsh @('-NoProfile','-File',(Join-Path $PSScriptRoot 'candidate-smoke.ps1'),'-NativeResult',$run.receipts.native,'-SetupResult',$run.receipts.setup,'-OutputRoot',(Join-Path $private 'native'),'-ConsoleTestExecutable',$consoleTest)
    }
    Stage 'installed-editor' @('cargo','+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','beta_editor_candidate','--','--ignored','--nocapture','--test-threads=1') 'Synthetic both-store history, reload/restart, incompatible or missing engine and rejected update through actual installed VSIX and production engine; no live calls.' {
        $env:VCP_BETA_NATIVE_RESULT=$run.receipts.native; $env:VCP_BETA_SETUP_RESULT=$run.receipts.setup; $env:VCP_BETA_VSIX_MANIFEST=$run.receipts.vsix; $env:VCP_TEST_CODE=Join-Path $editor 'Code.exe'
        Push-Location $workspace
        try { Checked 'cargo' @('+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','beta_editor_candidate','--','--ignored','--nocapture','--test-threads=1') } finally { Pop-Location }
    }
    $run.status='pass'
} catch { $run.status='fail'; $run.failure=$_.Exception.Message }
finally {
    # A failed production build can still emit a reviewable receipt.
    if (-not $run.receipts.build -and (Test-Path -LiteralPath (Join-Path $out 'build'))) {
        $failed = @(Get-ChildItem -LiteralPath (Join-Path $out 'build') -Filter build-receipt.json -Recurse -File)
        if ($failed.Count -eq 1) { $run.receipts.build=$failed[0].FullName }
    }
    $run.ended_at=[DateTime]::UtcNow.ToString('o'); Save-Run
}
Write-Output $runPath
if ($run.status -ne 'pass') { Write-Error $run.failure; exit 1 }
