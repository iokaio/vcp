# SPDX-License-Identifier: Apache-2.0
#Requires -Version 7.4
<#
.SYNOPSIS
Run the synthetic U04 machine-B handoff through production CLI commands.
.EXAMPLE
pwsh -File .\p3-portability-machine-b.ps1 -PackageRoot E:\VCP-U04 -RecoveryRoot F:\VCP-Recovery -OperatorConfirmsActualOneDrive
.NOTES
Package/trust anchors and recovery material arrive independently of OneDrive.
Only ciphertext is read/published through the declared vault. No provider/model
command is issued. Partial runs are retained and refused on rerun; reconcile the
recorded CLI operation before choosing a fresh local root. Never recursively deletes.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageRoot,
    [Parameter(Mandatory)][string]$RecoveryRoot,
    [string]$OneDriveRoot,
    [string]$LocalRoot,
    [string]$Git,
    [switch]$OperatorConfirmsActualOneDrive,
    [switch]$LocalSimulation,
    [ValidateRange(30,900)][int]$CommandTimeoutSeconds=300
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
if (-not $IsWindows) { throw 'Windows and PowerShell 7.4 or newer are required.' }
if ($LocalSimulation -and $OperatorConfirmsActualOneDrive) { throw 'Simulation and real OneDrive confirmation are mutually exclusive.' }
if (-not $LocalSimulation -and -not $OperatorConfirmsActualOneDrive) { throw 'Confirm the real OneDrive arrangement with -OperatorConfirmsActualOneDrive, or explicitly select -LocalSimulation.' }

function Hash-Text([string]$Text) {
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text))).ToLowerInvariant()
}
function Ordinary-Path([string]$Path,[bool]$Directory) {
    $item=Get-Item -LiteralPath $Path -Force
    if ([bool]$item.PSIsContainer -ne $Directory) { throw 'Unexpected file/directory type.' }
    $ancestor=$item
    while ($null -ne $ancestor) {
        if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected or cloud-placeholder path is not qualified; hydrate/reconcile through the documented native boundary.' }
        $ancestor=if($ancestor -is [IO.DirectoryInfo]){$ancestor.Parent}else{$ancestor.Directory}
    }
    $full=[IO.Path]::GetFullPath($item.FullName)
    if($full.Length -gt [IO.Path]::GetPathRoot($full).Length){$full=$full.TrimEnd('\','/')}
    $full
}
function Relative-Path([string]$Root,[string]$Relative) {
    if ([string]::IsNullOrWhiteSpace($Relative) -or [IO.Path]::IsPathRooted($Relative) -or $Relative.Contains(':') -or $Relative.Contains('\')) { throw 'Invalid portable relative path.' }
    foreach($part in $Relative.Split('/')) {
        if ($part -in @('','.','..') -or $part.TrimEnd(' ','.') -ne $part -or $part.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0) { throw 'Unsafe relative path component.' }
    }
    Join-Path $Root $Relative
}
function Overlaps([string]$A,[string]$B) {
    $A=$A.TrimEnd('\','/'); $B=$B.TrimEnd('\','/')
    $A.Equals($B,[StringComparison]::OrdinalIgnoreCase) -or $A.StartsWith($B+'\',[StringComparison]::OrdinalIgnoreCase) -or $B.StartsWith($A+'\',[StringComparison]::OrdinalIgnoreCase)
}
function File-Proof([string]$Path,[long]$Maximum) {
    $path=Ordinary-Path $Path $false
    $stream=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        if ($stream.Length -lt 1 -or $stream.Length -gt $Maximum) { throw 'File exceeds qualification size bounds.' }
        $sha=[Security.Cryptography.SHA256]::Create()
        try { $digest=[Convert]::ToHexString($sha.ComputeHash($stream)).ToLowerInvariant() } finally { $sha.Dispose() }
        @{sha256=$digest;bytes=$stream.Length}
    } finally { $stream.Dispose() }
}
function Save-New([string]$Path,$Value) {
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($Value|ConvertTo-Json -Depth 40))
    if ($bytes.Length -gt 4MB) { throw 'Public receipt exceeds bound.' }
    $stream=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $stream.Write($bytes); $stream.Flush($true) } finally { $stream.Dispose() }
}
function Selected-Proof($Value) { @($Value.key_ref,$Value.recipient,($Value.writer|ConvertTo-Json -Compress))|ConvertTo-Json -Compress }

$PackageRoot=Ordinary-Path $PackageRoot $true
$RecoveryRoot=Ordinary-Path $RecoveryRoot $true
if(Overlaps $PackageRoot $RecoveryRoot){throw 'Recovery material must be in a separate nonoverlapping root, outside the ordinary package.'}
$manifestPath=Ordinary-Path (Join-Path $PackageRoot 'package.json') $false
if ((Get-Item -LiteralPath $manifestPath).Length -gt 1MB) { throw 'Package manifest exceeds bound.' }
$manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json
if ($manifest.schema_version -ne 1 -or $manifest.campaign -notmatch '^u04-[a-f0-9-]{36}$' -or $manifest.build_commit -notmatch '^[a-f0-9]{40}$' -or $manifest.machine_a -notmatch '^[a-f0-9]{64}$' -or @($manifest.cases).Count -ne 2) { throw 'Unsupported or malformed U04 package manifest.' }
$manifestHash=(File-Proof $manifestPath 1MB).sha256
$vcp=Ordinary-Path (Relative-Path $PackageRoot $manifest.executable.path) $false
$binaryProof=File-Proof $vcp 512MB
if ($binaryProof.sha256 -ne $manifest.executable.sha256 -or $binaryProof.bytes -ne $manifest.executable.bytes) { throw 'Executable does not match independently supplied package manifest.' }
if (-not $Git) { $Git=(Get-Command git.exe -CommandType Application -ErrorAction Stop).Source }
$Git=Ordinary-Path $Git $false
if (Overlaps $Git $PackageRoot) { throw 'Select an independently installed Git for Windows executable.' }
if (-not $OneDriveRoot) {
    $candidates=@(@($env:OneDrive,$env:OneDriveConsumer,$env:OneDriveCommercial)|Where-Object {$_ -and (Test-Path -LiteralPath $_)}|Sort-Object -Unique)
    if ($candidates.Count -ne 1) { throw 'Select the intended OneDrive root explicitly with -OneDriveRoot.' }
    $OneDriveRoot=$candidates[0]
}
$OneDriveRoot=[IO.Path]::GetFullPath($OneDriveRoot).TrimEnd('\','/')
if(-not(Test-Path -LiteralPath $OneDriveRoot -PathType Container)){throw 'Selected OneDrive directory must already exist.'}
foreach($private in @($PackageRoot,$RecoveryRoot)) { if (Overlaps $private $OneDriveRoot) { throw 'Package/trust anchors and recovery material must arrive outside the sync root.' } }
$knownSync=@($OneDriveRoot)+@(@($env:OneDrive,$env:OneDriveConsumer,$env:OneDriveCommercial)|Where-Object {$_ -and (Test-Path -LiteralPath $_)}|ForEach-Object{[IO.Path]::GetFullPath($_).TrimEnd('\','/')})
if (-not $LocalRoot) { $LocalRoot=Join-Path $env:LOCALAPPDATA ('VCP/U04/'+$manifest.campaign+'/B') }
$LocalRoot=[IO.Path]::GetFullPath($LocalRoot).TrimEnd('\','/')
foreach($other in @($PackageRoot,$RecoveryRoot)+$knownSync) { if (Overlaps $LocalRoot $other) { throw 'Private machine-B state must be separated from package, recovery and synchronization roots.' } }
$machineGuid=(Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid).MachineGuid
$machine=Hash-Text ($manifest.campaign+':'+$machineGuid); $machineGuid=$null
if (-not $LocalSimulation -and $machine -eq $manifest.machine_a) { throw 'Real U04 requires a distinct second Windows installation.' }
$oneDriveVersion=$null
if (-not $LocalSimulation) {
    $client=Get-Process -Name OneDrive -ErrorAction Stop|Select-Object -First 1
    $oneDriveVersion=$client.MainModule.FileVersionInfo.FileVersion
}
if (Test-Path -LiteralPath $LocalRoot) {
    $null=Ordinary-Path $LocalRoot $true
    $receiptPath=Join-Path $LocalRoot 'machine-b-receipt.json'
    if (Test-Path -LiteralPath $receiptPath) {
        if((Get-Item -LiteralPath $receiptPath).Length -gt 4MB){throw 'Retained completion receipt exceeds bound.'}
        $old=Get-Content -LiteralPath (Ordinary-Path $receiptPath $false) -Raw|ConvertFrom-Json
        if ($old.package_sha256 -eq $manifestHash -and $old.machine_b -eq $machine -and $old.local_simulation -eq [bool]$LocalSimulation) { $old|ConvertTo-Json -Depth 40; return }
    }
    throw "Existing machine-B run retained at $LocalRoot. Inspect step journals and reconcile its exact CLI operation; this script will not overwrite or silently restart it."
}
# Verify the nearest existing ancestor before creating private children.
$ancestor=$LocalRoot
while (-not(Test-Path -LiteralPath $ancestor)) { $ancestor=[IO.Path]::GetDirectoryName($ancestor); if(-not $ancestor){throw 'Local root has no existing parent.'} }
$null=Ordinary-Path $ancestor $true
New-Item -ItemType Directory -Path $LocalRoot -Force|Out-Null
$LocalRoot=Ordinary-Path $LocalRoot $true
$script:step=0
function Run-Native([string]$Executable,[string[]]$Arguments,[string]$Label,[bool]$Json) {
    $script:step++
    $prefix=Join-Path $LocalRoot ('{0:D3}-{1}' -f $script:step,$Label)
    Save-New ($prefix+'.intent.json') @{step=$script:step;label=$Label;at_utc=[DateTime]::UtcNow.ToString('o');arguments=$Arguments;executable_sha256=(File-Proof $Executable 512MB).sha256}
    $start=[Diagnostics.ProcessStartInfo]::new($Executable)
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.WorkingDirectory=$LocalRoot
    $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true; $start.RedirectStandardInput=$true
    foreach($arg in $Arguments){$start.ArgumentList.Add($arg)}
    foreach($name in @($start.Environment.Keys)){if($name -like 'VCP_TEST_*' -or $name -like 'GIT_*'){$null=$start.Environment.Remove($name)}}
    $start.Environment['GIT_CONFIG_NOSYSTEM']='1'
    $start.Environment['GIT_CONFIG_GLOBAL']='NUL'
    $process=[Diagnostics.Process]::Start($start); $process.StandardInput.Close()
    $stdout=[IO.File]::Open($prefix+'.stdout',[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    $stderr=[IO.File]::Open($prefix+'.stderr',[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    try {
        $outTask=$process.StandardOutput.BaseStream.CopyToAsync($stdout); $errTask=$process.StandardError.BaseStream.CopyToAsync($stderr)
        $deadline=[DateTime]::UtcNow.AddSeconds($CommandTimeoutSeconds)
        while (-not $process.WaitForExit(100)) {
            if ([DateTime]::UtcNow -ge $deadline -or $stdout.Length -gt 8MB -or $stderr.Length -gt 1MB) { $process.Kill($true); $process.WaitForExit(); throw "Native command stopped at bounded step $Label; reconcile its retained journal before retry." }
        }
        $null=$outTask.GetAwaiter().GetResult(); $null=$errTask.GetAwaiter().GetResult()
        if ($stdout.Length -gt 8MB -or $stderr.Length -gt 1MB) { throw 'Native output bound exceeded.' }
        if ($process.ExitCode -ne 0) { throw "Native command rejected at step $Label; inspect private local diagnostics. No later step ran." }
    } finally {
        if(-not $process.HasExited){$process.Kill($true);$process.WaitForExit()}
        $stdout.Dispose();$stderr.Dispose();$process.Dispose()
    }
    if ($Json) {
        $rows=@(Get-Content -LiteralPath ($prefix+'.stdout')|ForEach-Object{$_|ConvertFrom-Json}|Where-Object {$_.type -eq 'result'})
        if ($rows.Count -ne 1) { throw 'Expected one structured CLI result.' }
        return $rows[0].data
    }
}
function Vcp([string]$Data,[string]$Workspace,[string[]]$Arguments,[string]$Label) {
    Run-Native $vcp (@('--format','jsonl','--data-dir',$Data,'--workspace',$Workspace)+$Arguments) $Label $true
}
function Cloud-Proof([string]$Vault,[string]$Object,[string]$Phase,[string]$Backend,[long]$Sequence,[long]$Deletion,[string]$ExpectedHash,[long]$ExpectedBytes) {
    $collector=Ordinary-Path (Join-Path $PSScriptRoot 'p3-portability-handoff.ps1') $false
    $evidence=Join-Path $LocalRoot ('cloud-'+[guid]::NewGuid().ToString()+'.json')
    $arguments=@('-NoProfile','-File',$collector,'-Campaign',$manifest.campaign,'-Machine','B','-Phase',$Phase,'-Backend',$Backend,'-Commit',$manifest.build_commit,'-Vault',$Vault,'-Object',$Object,'-Evidence',$evidence,'-Sequence',[string]$Sequence,'-Deletion',[string]$Deletion)
    if($ExpectedHash){$arguments+=@('-ExpectedSha256',$ExpectedHash,'-ExpectedBytes',[string]$ExpectedBytes)}
    if($OperatorConfirmsActualOneDrive){$arguments+='-OperatorConfirmsActualOneDrive'}
    $null=Run-Native (Join-Path $PSHOME 'pwsh.exe') $arguments $Phase $false
    $proof=Get-Content -LiteralPath (Ordinary-Path $evidence $false) -Raw|ConvertFrom-Json
    if($proof.observation -ne 'readable-ciphertext' -or $proof.machine_identity -ne $machine){throw 'Native cloud collector did not verify the expected object.'}
    @{sha256=$proof.ciphertext_sha256;bytes=$proof.bytes;observed_at_utc=$proof.recorded_at_utc}
}
function Inspect-All([string]$Data,[string]$Workspace,[string]$Task,[string]$View) {
    $items=@();$cursor=$null
    for($page=0;$page -lt 32;$page++) {
        $arguments=@('inspect',$Task,'--view',$View,'--limit','128')
        if($null -ne $cursor){$arguments+=@('--cursor',($cursor|ConvertTo-Json -Compress -Depth 20))}
        $value=Vcp $Data $Workspace $arguments ($View+'-inspection')
        if(@($value.gaps).Count -gt 0){throw 'Baseline inspection contains an explicit visibility gap.'}
        $items+=@($value.items);$cursor=$value.next_cursor
        if($null -eq $cursor){return ,$items}
    }
    throw 'Inspection exceeded its explicit paging bound; complete comparison separately.'
}

$results=@(); $seen=@{}
foreach($case in $manifest.cases) {
    if ($case.id -notin @('files-sqlite-files','sqlite-files-sqlite') -or $seen.ContainsKey($case.id)) { throw 'Unexpected or duplicate campaign case.' }; $seen[$case.id]=$true
    if ($case.destination_backend -notin @('files','sqlite') -or $case.source_backend -eq $case.destination_backend -or $case.snapshot.object -notmatch '^[a-f0-9-]{36}\.age$' -or $case.lineage -notmatch '^[a-f0-9]{64}$') { throw 'Invalid case identity.' }
    if($case.id -ne ($case.source_backend+'-'+$case.destination_backend+'-'+$case.source_backend) -or $case.vault_relative -cne ('VCP-U04/'+$manifest.campaign+'/'+$case.id)){throw 'Case backend direction or vault scope differs from the campaign.'}
    if ($case.snapshot.sequence -ne ($case.checkpoint.sequence+1) -or $case.snapshot.parent -ne $case.checkpoint.parent -or $case.snapshot.deletion -lt $case.checkpoint.deletion) { throw 'Independent snapshot lineage metadata is inconsistent.' }
    $vault=[IO.Path]::GetFullPath((Relative-Path $OneDriveRoot $case.vault_relative))
    if(-not(Test-Path -LiteralPath $vault -PathType Container)){throw 'Wait for the existing campaign vault to arrive through OneDrive.'}
    $object=Relative-Path $vault $case.snapshot.object
    $incoming=Cloud-Proof $vault $case.snapshot.object 'b-hydrated' $case.destination_backend $case.snapshot.sequence $case.snapshot.deletion $case.snapshot.sha256 $case.snapshot.bytes
    if ($incoming.sha256 -ne $case.snapshot.sha256 -or $incoming.bytes -ne $case.snapshot.bytes) { throw 'Hydrated OneDrive ciphertext differs from independent A metadata.' }
    $key=Ordinary-Path (Relative-Path $RecoveryRoot $case.key_file) $false
    if ([IO.Path]::GetExtension($key) -ne '.recovery') { throw 'Independent recovery file required.' }
    $caseRoot=Join-Path $LocalRoot $case.id
    $data=Join-Path $caseRoot 'data';$enrollment=Join-Path $caseRoot 'enrollment';$staging=Join-Path $caseRoot 'staging';$work=Join-Path $caseRoot 'workspace';$empty=Join-Path $caseRoot 'empty-git-template'
    foreach($dir in @($data,$enrollment,$staging,$empty)){New-Item -ItemType Directory -Path $dir -Force|Out-Null}
    $doctor=Vcp $data $enrollment @('doctor','--vault',$vault,'--staging',$staging,'--sync-root',$OneDriveRoot) 'doctor'
    if (-not $doctor.path_checks_passed) { throw 'Native path preflight rejected the real layout.' }
    $checkpoint=Join-Path $caseRoot 'independent-checkpoint.json';Save-New $checkpoint $case.checkpoint
    $imported=Vcp $data $enrollment @('backup','keys','--workspace-id',$case.workspace,'import','--key',$key,'--lineage',$case.lineage,'--checkpoint',$checkpoint,'--sync-root',$OneDriveRoot) 'enroll'
    if ((Selected-Proof $imported.configuration.selected) -cne (Selected-Proof $case.selected) -or $imported.configuration.lineage -ne $case.lineage) { throw 'Recovered public writer/recipient differs from independent enrollment.' }
    $restore=@('restore','--workspace-id',$case.workspace,'--source',$object,'--key',$key,'--staging',$staging,'--backend',$case.destination_backend,'--sync-root',$OneDriveRoot)
    $preview=Vcp $data $work ($restore+@('--preview')) 'restore-preview'
    if ($preview.expected_descriptor -or $preview.ciphertext_sha256 -ne $incoming.sha256 -or $preview.bytes -ne $incoming.bytes) { throw 'Fresh restore preview identity changed.' }
    $apply=$restore+@('--operation',$preview.operation,'--ciphertext-sha256',$preview.ciphertext_sha256,'--bytes',[string]$preview.bytes)
    $restored=Vcp $data $work $apply 'restore-apply'
    if (-not $restored.activated -or $restored.tasks_resumed -or $restored.rebind.trust -ne 'untrusted') { throw 'Restore did not preserve the untrusted paused boundary.' }
    if (Test-Path -LiteralPath (Join-Path $work '.git')) { throw 'Restored executable Git metadata is unexpected.' }
    if (@($case.expected_files).Count -lt 1 -or @($case.expected_files).Count -gt 4096) { throw 'Expected source inventory is out of bounds.' }
    foreach($file in $case.expected_files) {
        if ($file.path.Split('/') -contains '.git') { throw 'Git metadata is not a source fixture.' }
        $proof=File-Proof (Relative-Path $work $file.path) 64MB
        if ($proof.sha256 -ne $file.sha256 -or $proof.bytes -ne $file.bytes) { throw 'Restored dirty/untracked source bytes differ.' }
    }
    $taskPage=Vcp $data $work @('tasks','status',$case.root_task) 'task-status'
    if($taskPage.truncated){throw 'Task status was truncated.'}
    $tasks=@($taskPage.records)
    if ($tasks.Count -ne 1 -or $tasks[0].scope.workspace -ne $case.workspace -or $tasks[0].scope.session -ne $case.session -or $tasks[0].scope.task -ne $case.root_task -or $tasks[0].root -ne $case.root_task -or $tasks[0].state -ne 'paused') { throw 'Stable task/session/workspace identity or paused state differs.' }
    $costs=Inspect-All $data $work $case.root_task 'costs'
    $checks=Inspect-All $data $work $case.root_task 'verification'
    $baselineVerified=$false
    if($case.PSObject.Properties['baseline'] -and $case.baseline.PSObject.Properties['settled']) {
        $ledgers=@($costs|Where-Object {$_.collection -eq 'ledger' -and $_.id -eq $case.root_task -and $_.visibility -eq 'available'})
        if($ledgers.Count -ne 1 -or [string]$ledgers[0].record.settled -cne [string]$case.baseline.settled -or [string]$ledgers[0].record.unresolved -cne [string]$case.baseline.unresolved){throw 'Retained settled charges or unresolved liabilities differ from independent baseline.'}
        $childPage=Vcp $data $work @('tasks','status',$case.baseline.child_task) 'child-status'
        if($childPage.truncated){throw 'Child status was truncated.'}
        $child=@($childPage.records)
        if($child.Count -ne 1 -or $child[0].scope.task -ne $case.baseline.child_task -or $child[0].scope.workspace -ne $case.workspace -or $child[0].parent -ne $case.root_task -or $child[0].state -notin @('paused','completed','cancelled','failed')){throw 'Retained child identity, lineage or safe state differs.'}
        $claims=if($case.baseline.PSObject.Properties['claims']){@($case.baseline.claims)}elseif($case.baseline.PSObject.Properties['claim']){@($case.baseline.claim)}else{@()}
        if(@($claims).Count -lt 1 -or @($claims).Count -gt 32){throw 'Governed claim baseline is outside its explicit bound.'}
        foreach($claimId in $claims){
            $claim=Vcp $data $work @('memory','inspect',$claimId,'--limit','32') 'claim-inspection'
            if($claim.workspace -ne $case.workspace -or $claim.claim -ne $claimId -or @($claim.versions|Where-Object {$_.visibility -eq 'retained' -and $null -ne $_.version}).Count -eq 0 -or $null -ne $claim.next_cursor){throw 'Governed claim baseline is missing, pruned or requires additional paging.'}
        }
        $baselineVerified=$true
    }
    $retry=Vcp $data $work $apply 'restore-retry'
    if (-not $retry.reconciled -or -not $retry.rebind.already_completed) { throw 'Exact restore retry did not reconcile the accepted operation.' }
    $afterA=Vcp $data $work @('backup','keys','verify','--key',$key,'--sync-root',$OneDriveRoot) 'accepted-A-head'
    if ($afterA.configuration.checkpoint.sequence -ne $case.snapshot.sequence -or $afterA.configuration.checkpoint.deletion -ne $case.snapshot.deletion) { throw 'Accepted A checkpoint differs from independent expectation.' }
    $null=Run-Native $Git @('-c',"core.hooksPath=$empty",'init',"--template=$empty",$work) 'git-init' $false
    # Only independently enumerated synthetic fixture files become the baseline.
    $paths=@($case.expected_files|ForEach-Object{[string]$_.path})
    $null=Run-Native $Git (@('-C',$work,'-c',"core.hooksPath=$empty",'add','--')+$paths) 'git-add' $false
    $null=Run-Native $Git @('-C',$work,'-c',"core.hooksPath=$empty",'-c','user.name=VCP-U04','-c','user.email=u04@example.invalid','-c','commit.gpgSign=false','commit','-m','Reviewed synthetic U04 baseline') 'git-commit' $false
    $rebound=Vcp $data $work @('workspace','rebind',$case.workspace) 'git-rebind'
    $trusted=Vcp $data $work @('workspace','trust',$case.workspace,'--expected-revision',[string]$rebound.workspace_revision) 'explicit-trust'
    if ($trusted.policy_mode -ne 'plan' -or $trusted.tasks_resumed -or $trusted.provider_requests -ne 0) { throw 'Maintenance trust changed execution policy or resumed work.' }
    $markerName='u04-b-'+[guid]::NewGuid().ToString()+'.txt'
    [IO.File]::WriteAllText((Join-Path $work $markerName),"Synthetic U04 B marker: $($manifest.campaign) / $($case.id)`n",[Text.UTF8Encoding]::new($false))
    $marker=File-Proof (Join-Path $work $markerName) 4096
    $null=Vcp $data $work @('backup','configure','--vault',$vault,'--staging',$staging,'--sync-root',$OneDriveRoot,'--manual-only') 'configure-B'
    $operation=[guid]::NewGuid().ToString()
    $published=Vcp $data $work @('backup','create','--key',$key,'--git',$Git,'--operation',$operation) 'publish-B'
    if ($published.phase -ne 'finished') { throw 'Standalone publication did not complete.' }
    $afterB=Vcp $data $work @('backup','keys','verify','--key',$key,'--sync-root',$OneDriveRoot) 'accepted-B-head'
    if ($afterB.configuration.checkpoint.sequence -ne ($case.snapshot.sequence+1) -or $afterB.configuration.checkpoint.deletion -lt $case.snapshot.deletion) { throw 'B publication checkpoint is not the expected descendant.' }
    $outgoing=Cloud-Proof $vault ($operation+'.age') 'b-published' $case.destination_backend $afterB.configuration.checkpoint.sequence $afterB.configuration.checkpoint.deletion '' 0
    $results+=@{id=$case.id;workspace=$case.workspace;session=$case.session;root_task=$case.root_task;backend=$case.destination_backend;vault_relative=$case.vault_relative;incoming=$incoming;source_files_verified=@($case.expected_files).Count;paused_untrusted_restore_verified=$true;exact_retry_verified=$true;plan_preserved=$true;baseline_accounting_child_claim_verified=$baselineVerified;verification_records=@($checks).Count;marker=@{path=$markerName;sha256=$marker.sha256;bytes=$marker.bytes};snapshot=@{object=$operation+'.age';sha256=$outgoing.sha256;bytes=$outgoing.bytes;sequence=$afterB.configuration.checkpoint.sequence;deletion=$afterB.configuration.checkpoint.deletion;parent=$afterA.configuration.checkpoint.parent;manifest=$afterB.configuration.checkpoint.parent};search=$restored.search;baseline_comparison='bounded private inspections retained; A must verify returned history and provider delivery'}
}
$receipt=@{schema_version=1;campaign=$manifest.campaign;package_sha256=$manifestHash;build_commit=$manifest.build_commit;executable_sha256=$binaryProof.sha256;machine_a=$manifest.machine_a;machine_b=$machine;windows_version=[Environment]::OSVersion.Version.ToString();powershell=$PSVersionTable.PSVersion.ToString();onedrive_version=$oneDriveVersion;actual_onedrive_operator_assertion=[bool]$OperatorConfirmsActualOneDrive;local_simulation=[bool]$LocalSimulation;recorded_at_utc=[DateTime]::UtcNow.ToString('o');cases=$results;provider_requests=0;b_publication='local ciphertext publication verified; observe desktop-client upload separately';full_u04_qualified=$false}
Save-New (Join-Path $LocalRoot 'machine-b-receipt.json') $receipt
$receipt|ConvertTo-Json -Depth 40
Write-Host "Machine B completed. Wait for OneDrive to report upload complete, then return $LocalRoot\machine-b-receipt.json to A. Keep private journals and all roots; no cleanup was performed."
