# SPDX-License-Identifier: Apache-2.0
# Explicit final-installed configuration refusal observations. Never installs,
# removes an installation or admits provider/model work.
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$NativeResult,
    [Parameter(Mandatory)][string]$InstalledEngine,
    [Parameter(Mandatory)][string]$Node,
    [Parameter(Mandatory)][string]$QualificationExecutable,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$QualificationSha256,
    [Parameter(Mandatory)][string]$OutputRoot,
    [string[]]$SyncRoots=@()
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
. (Join-Path $PSScriptRoot 'build-progress.ps1')
. (Join-Path $PSScriptRoot '../evals/production-package.ps1')

function Get-ConfigRefusalHash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() }
function Assert-ConfigRefusalPath([string]$Path,[ValidateSet('file','directory','new')][string]$Kind,[switch]$SyncExclusion) {
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '[\x00-\x1f]') { throw 'Absolute ordinary local-drive path required' }
    $normal=$Path.Replace('/','\')
    if ($normal.Length -gt 3) { $normal=$normal.TrimEnd('\') }
    foreach ($part in $normal.Substring(3).Split('\')) {
        if ($normal.Length -gt 3 -and (-not $part -or $part -cin @('.','..') -or $part -match '[<>:"|?*\x00-\x1f]' -or
            $part -match '[. ]$' -or $part -match '^(?i:con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)')) { throw 'Unsafe path component refused' }
    }
    $full=[IO.Path]::GetFullPath($normal)
    if ($full.TrimEnd('\') -ine $normal.TrimEnd('\')) { throw 'Noncanonical path components refused' }
    for ($cursor=$full;$cursor;$cursor=[IO.Path]::GetDirectoryName($cursor)) {
        $entry=Get-Item -LiteralPath $cursor -Force -ErrorAction SilentlyContinue
        if ($entry) {
            if (-not $SyncExclusion -and ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Redirected qualification path refused' }
            if ($cursor -ine $full -and -not $entry.PSIsContainer) { throw 'Non-directory path ancestor refused' }
        }
    }
    $entry=Get-Item -LiteralPath $full -Force -ErrorAction SilentlyContinue
    if ($Kind -ceq 'new') { if ($entry) { throw 'Fresh private output required' } }
    elseif (-not $entry -or [bool]$entry.PSIsContainer -ne ($Kind -ceq 'directory')) { throw 'Required ordinary input has the wrong file type or is missing' }
    return $full
}
function Test-ConfigRefusalOverlap([string]$Left,[string]$Right) {
    $Left=$Left.TrimEnd('\');$Right=$Right.TrimEnd('\')
    return $Left -ieq $Right -or $Left.StartsWith($Right+'\',[StringComparison]::OrdinalIgnoreCase) -or $Right.StartsWith($Left+'\',[StringComparison]::OrdinalIgnoreCase)
}
function Assert-ConfigRefusalPrivateRoot([string]$Path,[string[]]$DeclaredSyncRoots) {
    $root=Assert-ConfigRefusalPath $Path 'new'
    for ($cursor=$root;$cursor;$cursor=[IO.Path]::GetDirectoryName($cursor)) {
        if (Test-Path -LiteralPath (Join-Path $cursor '.git')) { throw 'Private output must be outside repository trees' }
    }
    foreach ($selected in @(Get-ConfigRefusalSyncDeclarations $DeclaredSyncRoots)) {
        if (Test-ConfigRefusalOverlap $root $selected) { throw 'Private output overlaps a known or declared sync root' }
    }
    return $root
}
function Get-ConfigRefusalSyncDeclarations([string[]]$DeclaredSyncRoots) {
    $knownSync=@($DeclaredSyncRoots)
    foreach ($name in @('OneDrive','OneDriveConsumer','OneDriveCommercial','VCP_TEST_SYNC_ROOT')) {
        $value=[Environment]::GetEnvironmentVariable($name,'Process');if ($value) { $knownSync+=$value }
    }
    foreach ($sync in $knownSync) {
        # Exclusions may themselves be cloud-managed reparse directories or
        # aliases. This exception never applies to output, artifacts or tools.
        $kind=if (Get-Item -LiteralPath $sync -Force -ErrorAction SilentlyContinue) { 'directory' } else { 'new' }
        Assert-ConfigRefusalPath $sync $kind -SyncExclusion
    }
}
function Resolve-ConfigRefusalSyncExclusions([string]$Root,[string[]]$Declarations,[string]$NodeExecutable) {
    $nodeFile=Assert-ConfigRefusalPath $NodeExecutable 'file'
    foreach ($declared in $Declarations) {
        # Resolve the nearest existing ancestor as well as a present root. A
        # dangling link is present to lstat but cannot resolve and is refused.
        $code='const fs=require("node:fs"),path=require("node:path");let current=process.argv[1],suffix=[];for(;;){try{fs.lstatSync(current);break;}catch(e){if(e.code!=="ENOENT")throw e;const parent=path.dirname(current);if(parent===current)throw e;suffix.unshift(path.basename(current));current=parent;}}if(!fs.statSync(current).isDirectory())throw Error("Sync ancestor must be a directory");console.log(JSON.stringify({physical:path.join(fs.realpathSync.native(current),...suffix)}));'
        $resolved=Invoke-ConfigRefusalEnvironment @{} {
            $info=[Diagnostics.ProcessStartInfo]::new($nodeFile)
            $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
            $info.WorkingDirectory=Split-Path -Parent $nodeFile
            foreach ($argument in @('-e',$code,$declared)) { $info.ArgumentList.Add($argument) }
            $process=[Diagnostics.Process]::new();$process.StartInfo=$info;$started=$false
            try {
                $started=$process.Start();if (-not $started) { throw 'Sync identity resolver did not start' }
                $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
                if (-not $process.WaitForExit(10000)) { throw 'Sync identity resolution exceeded its deadline' }
                $null=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdout,$stderr)).WaitAsync([TimeSpan]::FromSeconds(3)).GetAwaiter().GetResult()
                if ($process.ExitCode -ne 0 -or $stdout.Result.Length -gt 8192 -or $stderr.Result.Length -gt 8192) { throw 'Sync identity resolution failed' }
                $stdout.Result | ConvertFrom-Json -ErrorAction Stop
            } finally {
                if ($started -and -not $process.HasExited) { $process.Kill($true);$null=$process.WaitForExit(5000) }
                $process.Dispose()
            }
        }
        if ($resolved.physical -isnot [string]) { throw 'Missing physical sync identity' }
        $kind=if (Get-Item -LiteralPath $resolved.physical -Force -ErrorAction SilentlyContinue) { 'directory' } else { 'new' }
        $physical=Assert-ConfigRefusalPath $resolved.physical $kind -SyncExclusion
        if ((Test-ConfigRefusalOverlap $Root $declared) -or (Test-ConfigRefusalOverlap $Root $physical)) { throw 'Private output overlaps a declared or resolved physical sync root' }
        @{declared=$declared;physical=$physical}
    }
}
function Invoke-ConfigRefusalEnvironment([hashtable]$Selected,[scriptblock]$Body) {
    $prior=[Environment]::GetEnvironmentVariables('Process')
    try {
        foreach ($name in @($prior.Keys)) { [Environment]::SetEnvironmentVariable($name,[NullString]::Value,'Process') }
        foreach ($name in @('SystemRoot','WINDIR','USERPROFILE','LOCALAPPDATA','APPDATA','ProgramFiles','ProgramFiles(x86)')) {
            if ($prior[$name]) { [Environment]::SetEnvironmentVariable($name,$prior[$name],'Process') }
        }
        [Environment]::SetEnvironmentVariable('PATH',($prior['SystemRoot']+'\System32;'+$prior['SystemRoot']),'Process')
        foreach ($name in $Selected.Keys) { [Environment]::SetEnvironmentVariable($name,$Selected[$name],'Process') }
        & $Body
    } finally {
        foreach ($name in @([Environment]::GetEnvironmentVariables('Process').Keys)) { [Environment]::SetEnvironmentVariable($name,[NullString]::Value,'Process') }
        foreach ($name in $prior.Keys) { [Environment]::SetEnvironmentVariable($name,$prior[$name],'Process') }
    }
}
function Save-ConfigRefusalReport([string]$Path,$Value) { $Value | ConvertTo-Json -Depth 70 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM }
function Get-ConfigRefusalTree([string]$Directory) {
    $root=Assert-ConfigRefusalPath $Directory 'directory'
    $rows=[Collections.Generic.List[object]]::new();$pending=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $pending.Push([IO.DirectoryInfo]::new($root));[long]$total=0
    while ($pending.Count) {
        foreach ($entry in $pending.Pop().GetFileSystemInfos()) {
            $kind=if ($entry -is [IO.DirectoryInfo]) { 'directory' } else { 'file' }
            $null=Assert-ConfigRefusalPath $entry.FullName $kind
            $relative=[IO.Path]::GetRelativePath($root,$entry.FullName).Replace('\','/')
            if ($kind -ceq 'directory') { $rows.Add([ordered]@{path=$relative;kind=$kind});$pending.Push($entry) }
            else {
                $total+=$entry.Length
                if ($entry.Length -gt 8MB -or $total -gt 32MB) { throw 'Verifier dependency inventory exceeds byte bound' }
                $rows.Add([ordered]@{path=$relative;kind=$kind;bytes=$entry.Length;sha256=(Get-ConfigRefusalHash $entry.FullName)})
            }
            if ($rows.Count -gt 1024) { throw 'Verifier dependency inventory exceeds entry bound' }
        }
    }
    return @($rows | Sort-Object { $_['path'] } -CaseSensitive)
}
function Assert-ConfigRefusalAggregate($Raw,[string]$EngineHash,[string]$QualificationHash,[string]$PowerShellHash,[string]$NodeHash) {
    if ($Raw.schema -cne 'vcp-final-import-refusals/1' -or $Raw.status -cne 'pass' -or
        $Raw.engine_sha256 -isnot [string] -or $Raw.engine_sha256 -cne $EngineHash -or
        $Raw.qualification_executable_sha256 -isnot [string] -or $Raw.qualification_executable_sha256 -cne $QualificationHash -or
        $Raw.powershell_sha256 -isnot [string] -or $Raw.powershell_sha256 -cne $PowerShellHash -or
        $Raw.node_sha256 -isnot [string] -or $Raw.node_sha256 -cne $NodeHash -or
        $Raw.controls -isnot [array] -or $Raw.controls.Count -ne 6 -or $Raw.cases -isnot [array] -or $Raw.cases.Count -ne 24) {
        throw 'Expected bound both-store aggregate with six controls and 24 refusal cases'
    }
    foreach ($family in @('controls','cases')) {
        $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        foreach ($row in $Raw.$family) {
            if ($row.backend -isnot [string] -or $row.backend -cnotin @('files','sqlite') -or $row.mode -isnot [string] -or
                $row.status -cne 'pass' -or $row.mcp_dispatched -isnot [bool] -or $row.mcp_dispatched -ne $false) { throw 'Invalid configuration observation or MCP dispatch' }
            foreach ($field in @('provider_attempts','reservations','send_intents')) {
                if (($row.$field -isnot [int] -and $row.$field -isnot [long]) -or $row.$field -ne 0) { throw 'Configuration observation admitted accounting or dispatch activity' }
            }
            if ($family -ceq 'controls') {
                if ($row.mode -cnotin @('cli','start','resume') -or $row.budget_exhausted -isnot [bool] -or $row.budget_exhausted -ne $true -or
                    -not $seen.Add($row.backend+':'+$row.mode)) { throw 'Controls must cover each store and entrypoint exactly once' }
            } else {
                if ($row.mode -cnotin @('start','resume') -or $row.change -isnot [string] -or
                    $row.change -cnotin @('base','revision','content','corrupt','removed','first-import') -or
                    $row.code -cne 'POLICY_DENIED' -or $row.immutable_records_preserved -isnot [bool] -or $row.immutable_records_preserved -ne $true -or
                    -not $seen.Add($row.backend+':'+$row.mode+':'+$row.change)) { throw 'Refusals must preserve state and cover every store, mode and configuration change exactly once' }
                if ($row.change -cin @('base','corrupt')) {
                    if (($row.cli_exit_code -isnot [int] -and $row.cli_exit_code -isnot [long]) -or $row.cli_exit_code -ne 2 -or
                        $row.reconnect_rejected -isnot [bool] -or $row.reconnect_rejected -ne $true) { throw 'Base/corrupt cases require actual CLI and reconnect refusals' }
                } elseif ($null -ne $row.cli_exit_code -or $null -ne $row.reconnect_rejected) { throw 'Unselected CLI or reconnect observation must remain null' }
            }
        }
    }
}

if (-not $IsWindows) { throw 'Native Windows qualification required' }
$root=Assert-ConfigRefusalPrivateRoot $OutputRoot $SyncRoots
$NativeResult=Assert-ConfigRefusalPath $NativeResult 'file'
$InstalledEngine=Assert-ConfigRefusalPath $InstalledEngine 'file'
$Node=Assert-ConfigRefusalPath $Node 'file'
$QualificationExecutable=Assert-ConfigRefusalPath $QualificationExecutable 'file'
if ((Get-ConfigRefusalHash $QualificationExecutable) -cne $QualificationSha256) { throw 'Selected qualification executable hash differs' }
$nodeHash=Get-ConfigRefusalHash $Node
$syncDeclarations=@(Get-ConfigRefusalSyncDeclarations $SyncRoots | Sort-Object -Unique)
$syncExclusions=@(Resolve-ConfigRefusalSyncExclusions $root $syncDeclarations $Node)
if ((Get-ConfigRefusalHash $Node) -cne $nodeHash) { throw 'Selected Node changed during sync identity resolution' }
$powershell=Assert-ConfigRefusalPath (Get-Process -Id $PID).Path 'file'
$releaseRoot=Split-Path -Parent $InstalledEngine
$archiveId=Split-Path -Leaf $releaseRoot
$engineRoot=Split-Path -Parent (Split-Path -Parent $releaseRoot)
$app=Split-Path -Parent $engineRoot
if ($archiveId -cnotmatch '^[a-f0-9]{64}$' -or $InstalledEngine -ine (Join-Path $app "engine/releases/$archiveId/vcp.exe")) { throw 'A real installed versioned engine path is required' }
$launcher=Assert-ConfigRefusalPath (Join-Path $app 'vcp.exe') 'file'
$ownerFile=Assert-ConfigRefusalPath (Join-Path $engineRoot '.vcp-install-owned.json') 'file'
if ((Get-Item -LiteralPath $ownerFile).Length -gt 65536) { throw 'Installation ownership metadata exceeds bound' }
$owner=Get-Content -LiteralPath $ownerFile -Raw | ConvertFrom-Json
if ($owner.schema -cne 'vcp-install-owned/1' -or $owner.install_root -ine $engineRoot) { throw 'Installed ownership differs from selected engine' }
$data=Assert-ConfigRefusalPath $owner.data_root 'directory'
if ((Test-ConfigRefusalOverlap $root $app) -or (Test-ConfigRefusalOverlap $root $data) -or (Test-ConfigRefusalOverlap $app $data)) { throw 'Private observations, installation and existing data must be disjoint' }
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$harnessPaths=@($PSCommandPath,(Join-Path $PSScriptRoot 'candidate-runtime.ps1'),(Join-Path $PSScriptRoot 'editor-layout.ps1'),
    (Join-Path $PSScriptRoot 'build-progress.ps1'),(Join-Path $PSScriptRoot 'provenance.cjs'),(Join-Path $PSScriptRoot '../package-inventory.cjs'),
    (Join-Path $PSScriptRoot 'notices.cjs'),(Join-Path $PSScriptRoot 'crates.cjs'),
    (Join-Path $PSScriptRoot '../evals/production-package.ps1'),(Join-Path $PSScriptRoot '../evals/production-package.cjs'),
    (Join-Path $repo 'src/crates/vcp-cli/tests/local_execution_parity.rs'))+
    @('final_import_refusals.rs','local_fixture.rs','import_execution.rs' | ForEach-Object { Join-Path $repo "src/crates/vcp-cli/tests/support/$_" })
$harness=@($harnessPaths | ForEach-Object { $file=Assert-ConfigRefusalPath ([IO.Path]::GetFullPath($_)) 'file';@{path=$file;sha256=(Get-ConfigRefusalHash $file)} })
$tomlRoot=Assert-ConfigRefusalPath (Join-Path $repo 'src/tests/node_modules/@iarna/toml') 'directory'
$tomlInventory=@(Get-ConfigRefusalTree $tomlRoot)
New-Item -ItemType Directory -Path $root | Out-Null
$temporary=Join-Path $root 'temporary';New-Item -ItemType Directory -Path $temporary | Out-Null
$rawRoot=Join-Path $root 'raw-observations'
$test='final_import_refusals::final_installed_configuration_changes_refuse_start_and_resume_on_both_stores'
$report=[ordered]@{schema='vcp-installed-config-refusals/1';status='fail';started_at=[DateTime]::UtcNow.ToString('o');
    qualification_executable=@{path=$QualificationExecutable;sha256=$QualificationSha256};
    node=@{path=$Node;sha256=$nodeHash};powershell=@{path=$powershell;sha256=(Get-ConfigRefusalHash $powershell)};
    harness=$harness;notice_parser=@{root=$tomlRoot;inventory=$tomlInventory};sync_exclusions=$syncExclusions;
    test=$test;deadline_seconds=300;command=@($QualificationExecutable,$test,'--exact','--ignored','--nocapture','--test-threads=1');
    authorization=@{maximum_live_provider_requests=0;maximum_model_calls=0};failures=@();
    limitations=@('Current Windows host and synthetic preserved both-store fixtures, not clean-host or paid-task acceptance.',
        'The invoking candidate runner must independently bind the exact native/setup/VSIX pair. This runner verifies the native package and existing registered installed selection only.',
        'Harness source hashes describe the current invocation; the supplied executable SHA256 binds the compiled test independently and is not a reproducible-build attestation.',
        'No independent operating-system network-denial observation. Live credentials are absent; fixtures use synthetic credentials and selected refusal cases must precede provider or MCP dispatch.',
        'No installation, uninstallation or private-state cleanup is performed; retain this output for diagnosis on failure.')}
$resultPath=Join-Path $root 'result.json';Save-ConfigRefusalReport $resultPath $report
$selectedEnvironment=@{TEMP=$temporary;TMP=$temporary;VCP_TEST_NODE=$Node;VCP_TEST_PWSH=$powershell;VCP_BETA_INSTALLED_EXECUTABLE=$InstalledEngine;
    VCP_BETA_IMPORT_OUTPUT=$rawRoot;RUST_MIN_STACK='16777216';CODEX_TEST_ENVIRONMENT='local';NO_COLOR='1'}
try {
    Invoke-ConfigRefusalEnvironment $selectedEnvironment {
        try {
            $candidate=Read-ProductionPackage -PackageResult $NativeResult -ExtractionRoot (Join-Path $root 'verified-package') -SelectedExecutable $InstalledEngine -NodeExecutable $Node
            if ($candidate.archive_sha256 -cne $archiveId) { throw 'Installed release directory differs from verified archive' }
            $registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1'
            $registered=(Get-ItemProperty -LiteralPath $registration -ErrorAction Stop).InstallLocation
            if ([IO.Path]::GetFullPath($registered).TrimEnd('\') -ine $app.TrimEnd('\')) { throw 'Matching per-user setup registration required' }
            $build=Get-Content -LiteralPath (Join-Path $releaseRoot 'build-receipt.json') -Raw | ConvertFrom-Json -Depth 60
            if ((Get-ConfigRefusalHash $launcher) -cne $build.launcher_sha256) { throw 'Installed launcher differs from verified build' }
            $selection=(Invoke-BetaProcess $launcher @('--resolve-installation') $root @{} 30).stdout | ConvertFrom-Json
            if ($selection.schema -cne 'vcp-installed-engine/1' -or
                (Assert-ConfigRefusalPath $selection.executable.Replace('\\?\','') 'file') -ine $InstalledEngine -or
                (Assert-ConfigRefusalPath $selection.data_directory.Replace('\\?\','') 'directory') -ine $data) { throw 'Registered launcher selects different engine or data' }
            $report.candidate=$candidate
            $report.installation=@{registration=$registration;registered_location=$registered;selection=$selection;
                launcher=@{path=$launcher;sha256=(Get-ConfigRefusalHash $launcher)};
                metadata=@('.vcp-install-owned.json','active.json' | ForEach-Object { $file=Assert-ConfigRefusalPath (Join-Path $engineRoot $_) 'file';@{path=$file;sha256=(Get-ConfigRefusalHash $file)} })}
            Save-ConfigRefusalReport $resultPath $report
            $log=Join-Path $root 'qualification.log'
            $report.supervision=Invoke-VcpBuildProcess -Executable $QualificationExecutable -Arguments @($test,'--exact','--ignored','--nocapture','--test-threads=1') -WorkingDirectory $root -LogPath $log -TimeoutSeconds 300
            Save-ConfigRefusalReport $resultPath $report
            if ($report.supervision.exit_code -ne 0 -or $report.supervision.process_exit_code -ne 0 -or $report.supervision.broker_exit_code -ne 0 -or
                $report.supervision.forced_cleanup -ne $false -or $report.supervision.job_active_processes_zero -ne $true -or
                $report.supervision.child_exit_observation_removed -ne $true) { throw 'Configuration refusal test did not finish naturally in its owned Job' }
            $text=Get-Content -LiteralPath $log -Raw
            if ([regex]::Matches($text,'(?m)^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; [0-9]+ filtered out; finished in [^\r\n]+\r?$').Count -ne 1 -or
                [regex]::Matches($text,'(?m)^running 1 test\r?$').Count -ne 1) { throw 'Exactly one ignored qualification test must actually pass' }
            $rawFile=Assert-ConfigRefusalPath (Join-Path $rawRoot 'result.json') 'file'
            if ((Get-Item -LiteralPath $rawFile).Length -gt 2MB) { throw 'Configuration observation aggregate exceeds bound' }
            $raw=Get-Content -LiteralPath $rawFile -Raw | ConvertFrom-Json -Depth 60
            Assert-ConfigRefusalAggregate $raw $candidate.executable_sha256 $QualificationSha256 $report.powershell.sha256 $report.node.sha256
            $report.observations=@{path=$rawFile;sha256=(Get-ConfigRefusalHash $rawFile);controls=$raw.controls;cases=$raw.cases}
        } catch { $report.failures+=@{phase='observations';reason=$_.Exception.Message} }
        finally {
            try {
                if ($report.installation) {
                    $null=Invoke-BetaProcess $Node @((Join-Path $PSScriptRoot '../evals/production-package.cjs'),$NativeResult,$releaseRoot) $root @{} 90
                    if ((Get-ConfigRefusalHash $NativeResult) -cne $report.candidate.receipt_sha256 -or
                        (Get-ConfigRefusalHash $report.candidate.archive) -cne $report.candidate.archive_sha256 -or
                        (Get-ConfigRefusalHash $launcher) -cne $report.installation.launcher.sha256) { throw 'Candidate artifact bytes changed during observations' }
                    $afterSelection=(Invoke-BetaProcess $launcher @('--resolve-installation') $root @{} 30).stdout | ConvertFrom-Json
                    if (($afterSelection | ConvertTo-Json -Compress) -cne ($report.installation.selection | ConvertTo-Json -Compress) -or
                        (Get-ItemProperty -LiteralPath $report.installation.registration -ErrorAction Stop).InstallLocation -cne $report.installation.registered_location) { throw 'Installed selection or registration changed' }
                    foreach ($file in $report.installation.metadata) {
                        $null=Assert-ConfigRefusalPath $file.path 'file'
                        if ((Get-ConfigRefusalHash $file.path) -cne $file.sha256) { throw 'Installation metadata changed' }
                    }
                    $report.complete_payload_and_selection_preserved=$true
                }
                foreach ($file in @($report.qualification_executable,$report.node,$report.powershell)+@($report.harness)) {
                    $null=Assert-ConfigRefusalPath $file.path 'file'
                    if ((Get-ConfigRefusalHash $file.path) -cne $file.sha256) { throw 'Selected tool or harness source changed' }
                }
                $afterToml=@(Get-ConfigRefusalTree $report.notice_parser.root)
                if (($afterToml | ConvertTo-Json -Depth 10 -Compress) -cne ($report.notice_parser.inventory | ConvertTo-Json -Depth 10 -Compress)) { throw 'Notice parser dependency bytes or inventory changed' }
                $report.harness_inputs_preserved=$true
            } catch { $report.failures+=@{phase='preservation';reason=$_.Exception.Message} }
        }
    } | Out-Null
    if ($report.failures.Count -or -not $report.observations -or $report.complete_payload_and_selection_preserved -ne $true -or $report.harness_inputs_preserved -ne $true) { throw 'Configuration refusal observations or preservation checks failed' }
    $report.status='pass'
} catch { $report.failure=$_.Exception.Message }
finally { $report.ended_at=[DateTime]::UtcNow.ToString('o');Save-ConfigRefusalReport $resultPath $report }
if ($report.status -cne 'pass') { throw "Configuration refusals failed; private evidence retained at $resultPath" }
Write-Output $resultPath
