# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Raw installed-helper observations; the ignored Rust target proves job quiescence.
param(
    [Parameter(Mandatory)][string]$NativeResult,[Parameter(Mandatory)][string]$InstalledEngine,
    [Parameter(Mandatory)][string]$Python,[Parameter(Mandatory)][string]$Node,
    [Parameter(Mandatory)][string]$BrowserProject,[Parameter(Mandatory)][string]$OutputRoot,
    [Parameter(Mandatory)][string]$QualificationExecutable,[string[]]$SyncRoots=@()
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
. (Join-Path $PSScriptRoot '../evals/production-package.ps1')
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Save([string]$Path,$Value) { $Value | ConvertTo-Json -Depth 60 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM }
function Require([bool]$Condition,[string]$Reason) { if(-not $Condition){throw $Reason} }
Require ($IsWindows -and [IO.Path]::IsPathFullyQualified($OutputRoot)) 'Native Windows and an absolute private output root required'
$root=[IO.Path]::GetFullPath($OutputRoot)
for($item=[IO.DirectoryInfo]::new($root);$null -ne $item;$item=$item.Parent){
    if(Test-Path -LiteralPath $item.FullName){
        $entry=Get-Item -LiteralPath $item.FullName -Force
        Require ($entry.PSIsContainer -and -not ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Private helper root has redirected ancestors'
    }
    Require (-not (Test-Path -LiteralPath (Join-Path $item.FullName '.git'))) 'Private helper output must be outside repository trees'
}
$knownSync=@($SyncRoots)
foreach($name in @('OneDrive','OneDriveConsumer','OneDriveCommercial')){
    $value=[Environment]::GetEnvironmentVariable($name);if($value){$knownSync+=$value}
}
foreach($sync in $knownSync){
    $selected=[IO.Path]::GetFullPath($sync).TrimEnd('\','/')
    Require (-not ($root.Equals($selected,[StringComparison]::OrdinalIgnoreCase) -or $root.StartsWith($selected+'\',[StringComparison]::OrdinalIgnoreCase) -or $selected.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase))) 'Private helper output overlaps a known or declared sync root'
}
Require (-not (Test-Path -LiteralPath $root)) 'Fresh private helper output required'
New-Item -ItemType Directory -Path $root | Out-Null
$temporary=Join-Path $root 'temporary';New-Item -ItemType Directory -Path $temporary | Out-Null
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$report=[ordered]@{schema='vcp-installed-helper-observations/1';status='fail';started_at=[DateTime]::UtcNow.ToString('o');
    qualification_executable_sha256=(Hash $QualificationExecutable);script_sha256=(Hash $PSCommandPath);
    environment=@{powershell=$PSVersionTable.PSVersion.ToString();os=[Environment]::OSVersion.VersionString};
    cases=@(@{id='pdf';status='not-run'},@{id='spreadsheet';status='not-run'},@{id='mcp';status='not-run'},@{id='authoring';status='not-run'},@{id='browser';status='not-run'});
    limitations=@('Synthetic helper fixtures and owned loopback browser interaction on this host; no model/provider work, clean-host claim or general skills campaign.',
        'The browser origin boundary is exercised, not an operating-system network-denial policy. Axe accessibility scanning is not selected.',
        'The Rust wrapper must additionally prove complete no-breakaway job termination; this raw observation receipt alone cannot do so.')}
$result=Join-Path $root 'result.json';Save $result $report
try {
    foreach($path in @($Node,$Python,$InstalledEngine,$QualificationExecutable)) { $null=Assert-BetaEditorPath $path $false }
    $null=Assert-BetaEditorPath $BrowserProject $true
    $powershell=Assert-BetaEditorPath (Get-Process -Id $PID).Path $false
    $report.powershell=@{path=$powershell;sha256=(Hash $powershell)}
    $candidate=Read-ProductionPackage -PackageResult $NativeResult -ExtractionRoot (Join-Path $root 'verified-package') -SelectedExecutable $InstalledEngine -NodeExecutable $Node
    $releaseRoot=Split-Path -Parent $candidate.executable
    $engineRoot=Split-Path -Parent (Split-Path -Parent $releaseRoot)
    $app=Split-Path -Parent $engineRoot
    Require ($candidate.executable -ieq (Join-Path $app "engine/releases/$($candidate.archive_sha256)/vcp.exe")) 'A real installed versioned engine path is required'
    $registered=(Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1').InstallLocation
    Require ([IO.Path]::GetFullPath($registered).TrimEnd('\') -ieq $app.TrimEnd('\')) 'Installed helper test requires matching per-user setup registration'
    $launcher=Assert-BetaEditorPath (Join-Path $app 'vcp.exe') $false
    $build=Get-Content -LiteralPath (Join-Path $releaseRoot 'build-receipt.json') -Raw | ConvertFrom-Json -Depth 60
    Require ((Hash $launcher) -ceq $build.launcher_sha256) 'Installed launcher differs from bound production receipt'
    $environment=@{TEMP=$temporary;TMP=$temporary;PYTHONDONTWRITEBYTECODE='1';PYTHONNOUSERSITE='1';PYTHONUTF8='1';PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD='1';XDG_CACHE_HOME=(Join-Path $temporary 'cache')}
    $selection=(Invoke-BetaProcess $launcher @('--resolve-installation') $root $environment 30).stdout | ConvertFrom-Json
    Require ($selection.schema -ceq 'vcp-installed-engine/1' -and [IO.Path]::GetFullPath($selection.executable).Replace('\\?\','') -ieq [IO.Path]::GetFullPath($candidate.executable).Replace('\\?\','')) 'Installed launcher selects a different native engine'
    $report.candidate=$candidate;$report.installed_launcher_sha256=Hash $launcher
    $report.installation=@{selection=$selection;registered_location=$registered;metadata=@(
        @('.vcp-install-owned.json','active.json') | ForEach-Object {$path=Join-Path $engineRoot $_;@{path=$path;sha256=(Hash $path)}})}
    $skills=Join-Path $releaseRoot 'skills/builtin';$environment.VCP_SKILLS_ROOT=$skills
    $pythonDriver=Join-Path $PSScriptRoot 'helper-python.py';$nodeDriver=Join-Path $PSScriptRoot 'helper-node.cjs'
    $harness=@($PSCommandPath,$pythonDriver,$nodeDriver,(Join-Path $PSScriptRoot 'candidate-runtime.ps1'),(Join-Path $PSScriptRoot 'editor-layout.ps1'),
        (Join-Path $PSScriptRoot 'provenance.cjs'),(Join-Path $PSScriptRoot '../package-inventory.cjs'),
        (Join-Path $PSScriptRoot '../evals/production-package.ps1'),(Join-Path $PSScriptRoot '../evals/production-package.cjs'),
        (Join-Path $repo 'src/crates/vcp-cli/tests/beta_helper_candidate.rs'),(Join-Path $repo 'src/crates/vcp-cli/tests/support/hidden_process.rs'))+
        @('test_pdf_workflows.py','test_spreadsheet_workflows.py','test_mcp_port.py','requirements-pdf-workflows.txt','requirements-spreadsheet-workflows.txt' | ForEach-Object {Join-Path $repo "src/tests/skills/$_"})
    $report.harness=@($harness | ForEach-Object {@{path=$_;sha256=(Hash $_)}})
    $report.helpers=@($candidate.manifest.files | Where-Object {$_.path -like 'skills/builtin/*'})
    $inputFile=Join-Path $root 'input.json'
    Save $inputFile @{skills=$skills;tests=(Join-Path $repo 'src/tests/skills');browser_project=$BrowserProject;output=$root}
    $pythonBefore=(Invoke-BetaProcess $Python @('-I','-B',$pythonDriver,$inputFile,'before') $root $environment 120).stdout | ConvertFrom-Json -Depth 30
    $nodeBefore=(Invoke-BetaProcess $Node @($nodeDriver,$inputFile,'before') $root $environment 120).stdout | ConvertFrom-Json -Depth 30
    $report.python_before=$pythonBefore;$report.node_before=$nodeBefore;Save $result $report
    foreach($case in $report.cases){
        $case.status='fail';Save $result $report
        if($case.id -in @('pdf','spreadsheet','mcp')){
            $observed=(Invoke-BetaProcess $Python @('-I','-B',$pythonDriver,$inputFile,$case.id) $root $environment 180).stdout | ConvertFrom-Json -Depth 30
        } else {
            $observed=(Invoke-BetaProcess $Node @($nodeDriver,$inputFile,$case.id) $root $environment 120).stdout | ConvertFrom-Json -Depth 40
        }
        Require ($observed.status -ceq 'pass') 'Installed helper observation failed'
        $case.observation=$observed;$case.status='pass';Save $result $report
    }
    $report.python_after=(Invoke-BetaProcess $Python @('-I','-B',$pythonDriver,$inputFile,'after') $root $environment 120).stdout | ConvertFrom-Json -Depth 30
    $report.node_after=(Invoke-BetaProcess $Node @($nodeDriver,$inputFile,'after') $root $environment 120).stdout | ConvertFrom-Json -Depth 30
    # Inventory filenames differ; compare every recorded input identity and tree digest.
    foreach($family in @('python','node')){
        $before=$report[$family+'_before'] | ConvertTo-Json -Depth 30 | ConvertFrom-Json -Depth 30
        $after=$report[$family+'_after'] | ConvertTo-Json -Depth 30 | ConvertFrom-Json -Depth 30
        foreach($item in @($before.trees)+@($after.trees)){$item.PSObject.Properties.Remove('inventory')}
        Require (($before | ConvertTo-Json -Depth 30 -Compress) -ceq ($after | ConvertTo-Json -Depth 30 -Compress)) 'Helper dependency bytes changed during observations'
    }
    $null=Invoke-BetaProcess $Node @((Join-Path $PSScriptRoot '../evals/production-package.cjs'),$NativeResult,$releaseRoot) $root $environment 90
    Require ((Hash $NativeResult) -ceq $candidate.receipt_sha256 -and (Hash $candidate.archive) -ceq $candidate.archive_sha256 -and (Hash $launcher) -ceq $report.installed_launcher_sha256) 'Final candidate changed during helper observations'
    $afterSelection=(Invoke-BetaProcess $launcher @('--resolve-installation') $root $environment 30).stdout | ConvertFrom-Json
    Require (($selection | ConvertTo-Json -Compress) -ceq ($afterSelection | ConvertTo-Json -Compress)) 'Installed launcher selection changed'
    Require ((Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1').InstallLocation -ceq $registered) 'Installed registration changed'
    foreach($file in $report.installation.metadata){Require ((Hash $file.path) -ceq $file.sha256) 'Installation metadata changed'}
    foreach($file in $report.harness){Require ((Hash $file.path) -ceq $file.sha256) 'Helper harness changed during observations'}
    Require ((Hash $powershell) -ceq $report.powershell.sha256) 'PowerShell runtime changed during observations'
    $report.complete_payload_before_after=$true;$report.dependencies_unchanged=$true;$report.status='observations-passed'
} catch {$report.failure=$_.Exception.Message}
finally {$report.ended_at=[DateTime]::UtcNow.ToString('o');Save $result $report}
if($report.status -cne 'observations-passed'){throw "Installed helper observations failed; private receipt retained at $result"}
$report | ConvertTo-Json -Depth 60
