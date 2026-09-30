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
    environment=@{os=[Runtime.InteropServices.RuntimeInformation]::OSDescription; architecture=[Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); node=(& node --version); powershell=$PSVersionTable.PSVersion.ToString(); runner_image=$env:ImageOS; runner_image_version=$env:ImageVersion; editor_archive_sha256=$EditorArchiveSha256; editor_version='1.138.0'; host='Windows build image with development tools; not clean standard-user qualification'}
}
function Save-Run { $run | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $runPath -Encoding utf8NoBOM }
function Stage([string]$Id,[string[]]$Command,[string]$Expected,[scriptblock]$Body) {
    $log = Join-Path $out "logs/$Id.log"
    $row = @{id=$Id;status='running';command=$Command;expected=$Expected;started_at=[DateTime]::UtcNow.ToString('o');log=$log;exit_code=$null}
    $run.stages += $row; Save-Run
    try {
        & $Body *> $log
        if ($Id -in @('native-boundaries','installed-editor')) {
            $text=Get-Content -LiteralPath $log -Raw
            $passed=0
            foreach ($match in [regex]::Matches($text,'test result: ok\. (\d+) passed')) { $passed += [int]$match.Groups[1].Value }
            $minimum=if ($Id -eq 'installed-editor') { 1 } else { 2 }
            if ($passed -lt $minimum) { throw 'Native command did not execute the required test cases' }
            $row.observed_passed=$passed
        }
        $row.status='pass'; $row.exit_code=0
    } catch {
        $row.status='fail'; $row.exit_code=1; $row.reason=$_.Exception.Message
        $_.Exception.Message | Add-Content -LiteralPath $log
        throw
    } finally { $row.ended_at=[DateTime]::UtcNow.ToString('o'); Save-Run }
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
$pwsh = (Get-Command pwsh -CommandType Application).Source
$node = (Get-Command node -CommandType Application).Source
$workspace = Join-Path $repository 'src/third_party/codex/codex-rs'
$channel = Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json
$private = Join-Path ([IO.Path]::GetTempPath()) ('vcp-beta-private-' + [guid]::NewGuid())
try {
    Stage 'source-gate' @('node','scripts/release/provenance.cjs','source',$repository,$ReviewedCommit) 'Clean exact reviewed source and channel versions.' {
        if ((& node --version) -cne ('v'+$tools.node)) { throw 'Candidate Node version differs from the reviewed tools pin' }
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
        $editorPackage = Get-Content -LiteralPath (Join-Path $editor 'resources/app/package.json') -Raw | ConvertFrom-Json
        if ($editorPackage.version -cne $channel.vscode_version) { throw 'Pinned editor version mismatch' }
        $editorProduct=Get-Content -LiteralPath (Join-Path $editor 'resources/app/product.json') -Raw | ConvertFrom-Json
        if ($editorProduct.commit -cne $tools.editor.commit) { throw 'Pinned editor commit mismatch' }
        $run.environment.editor_executable_sha256=(Get-FileHash -LiteralPath (Join-Path $editor 'Code.exe')).Hash.ToLowerInvariant()
    }
    Stage 'portable-contracts' @('npm.cmd','test','--prefix','src/packages/sdk-ts',';','npm.cmd','test','--prefix','src/packages/vscode',';','pwsh','-File','scripts/test.ps1','-Suite','fast') 'SDK/editor and fast contracts pass on the current Windows source.' {
        Checked 'npm.cmd' @('test','--prefix',(Join-Path $repository 'src/packages/sdk-ts'))
        Checked 'npm.cmd' @('test','--prefix',(Join-Path $repository 'src/packages/vscode'))
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/test.ps1'),'-Suite','fast')
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
    }
    Stage 'native-boundaries' @('cargo','+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','local_execution_parity','--test','installed_launcher','--','--test-threads=1') 'Separate qualification target: real CLI/client import parity and native launcher contract regressions.' {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
        foreach ($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin','Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')) { $env:PATH=(Join-Path $vsRoot $relative)+';'+$env:PATH }
        $env:VCP_TEST_NODE=$node; $env:VCP_TEST_GIT=(Get-Command git -CommandType Application).Source; $env:CODEX_TEST_ENVIRONMENT='local'; $env:RUST_MIN_STACK='16777216'
        Push-Location $workspace
        try { Checked 'cargo' @('+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','local_execution_parity','--test','installed_launcher','--','--test-threads=1') } finally { Pop-Location }
        Checked $pwsh @('-NoProfile','-File',(Join-Path $repository 'scripts/package-install.test.ps1'))
    }
    Stage 'installed-native' @('pwsh','-File','scripts/release/candidate-smoke.ps1','-NativeResult',$run.receipts.native,'-SetupResult',$run.receipts.setup,'-OutputRoot',(Join-Path $private 'native')) 'Install exact setup outside checkout, launch exact engine, preserve data and uninstall; hosted-image observation only.' {
        Checked $pwsh @('-NoProfile','-File',(Join-Path $PSScriptRoot 'candidate-smoke.ps1'),'-NativeResult',$run.receipts.native,'-SetupResult',$run.receipts.setup,'-OutputRoot',(Join-Path $private 'native'))
    }
    Stage 'installed-editor' @('cargo','+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $out 'qualification-target'),'-j',"$Jobs",'-p','vcp-cli','--features','qualification','--test','beta_editor_candidate','--','--ignored','--nocapture','--test-threads=1') 'Synthetic both-store history observed through actual installed VSIX and exact installed production engine; no live calls.' {
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
