# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# OutputRoot must be a NEW private directory outside repository and sync trees
# (normally Join-Path ([IO.Path]::GetTempPath()) ('vcp-production-recovery-'+[guid]::NewGuid())).
# It holds active plaintext history and synthetic workspaces. Copy only selected
# public receipts to repository artifacts after execution; preserve the private run.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$ExpectedSha256,
    [Parameter(Mandatory)][string]$FixtureRoot,
    [Parameter(Mandatory)][string]$OutputRoot,
    [string]$SyntheticProfile,
    [Parameter(Mandatory)][string]$GitExecutable,
    [Parameter(Mandatory)][string]$AgeExecutable,
    [Parameter(Mandatory)][string]$NodeExecutable,
    [Parameter(Mandatory)][string]$EnvelopeVerifier,
    [string]$RollbackExecutable,
    [string]$RollbackSha256,
    [string[]]$SyncRoots = @(),
    [int]$DeadlineSeconds = 180
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$exe = (Resolve-Path -LiteralPath $Executable).Path
$fixtures = (Resolve-Path -LiteralPath $FixtureRoot).Path
$git=(Resolve-Path -LiteralPath $GitExecutable).Path
$age=(Resolve-Path -LiteralPath $AgeExecutable).Path
$node=(Resolve-Path -LiteralPath $NodeExecutable).Path
$verifier=(Resolve-Path -LiteralPath $EnvelopeVerifier).Path
$rollback=$null
if ([bool]$RollbackExecutable -ne [bool]$RollbackSha256) { throw 'RollbackExecutable and RollbackSha256 must be supplied together' }
if ($RollbackExecutable) {
    if ($RollbackSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'RollbackSha256 must be a lowercase SHA-256 digest' }
    $rollback=(Resolve-Path -LiteralPath $RollbackExecutable).Path
    if ((Get-FileHash -LiteralPath $rollback).Hash.ToLowerInvariant() -cne $RollbackSha256) { throw 'Prior debug rollback executable digest differs' }
}
if ((Get-FileHash -LiteralPath $age).Hash.ToLowerInvariant() -cne '2821a4ed191da07372acd302e5f6feae7a7985e285e1417765ebe74025af45f0') { throw 'Independent Go age differs from pinned v1.3.2' }
$root = [IO.Path]::GetFullPath($OutputRoot)
for ($ancestor=[IO.DirectoryInfo]::new($root); $null -ne $ancestor; $ancestor=$ancestor.Parent) {
    if (Test-Path -LiteralPath $ancestor.FullName) {
        $entry=Get-Item -LiteralPath $ancestor.FullName -Force
        if (-not $entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'OutputRoot must have ordinary directory ancestors without reparse points' }
    }
    if (Test-Path -LiteralPath (Join-Path $ancestor.FullName '.git')) { throw 'OutputRoot must be private and outside repository trees; use a fresh system TEMP directory' }
}
$forbiddenSyncRoots=@($SyncRoots)
foreach ($syncName in @('OneDrive','OneDriveConsumer','OneDriveCommercial')) {
    $syncRoot=[Environment]::GetEnvironmentVariable($syncName)
    if ($syncRoot) { $forbiddenSyncRoots += $syncRoot }
}
foreach ($syncRoot in $forbiddenSyncRoots) {
    $syncPath=[IO.Path]::GetFullPath($syncRoot).TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar)
    $rootPath=$root.TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar)
    if ($rootPath.Equals($syncPath,[StringComparison]::OrdinalIgnoreCase) -or
        $rootPath.StartsWith(($syncPath+[IO.Path]::DirectorySeparatorChar),[StringComparison]::OrdinalIgnoreCase) -or
        $syncPath.StartsWith(($rootPath+[IO.Path]::DirectorySeparatorChar),[StringComparison]::OrdinalIgnoreCase)) {
        throw 'OutputRoot overlaps a declared or known OneDrive synchronization root'
    }
}
if (Test-Path -LiteralPath $root) { throw 'New output directory required' }
if ((Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -cne $ExpectedSha256) { throw 'Candidate hash differs' }
if ($DeadlineSeconds -lt 1 -or $DeadlineSeconds -gt 300) { throw 'Deadline must be 1..300 seconds' }
New-Item -ItemType Directory -Path $root | Out-Null
$report = [ordered]@{
    schema='vcp-production-local-recovery/1'; status='running'; executable_sha256=$ExpectedSha256
    script_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
    created_at=[DateTime]::UtcNow.ToString('o'); fixtures=@(); steps=@(); backends=@()
    rollback_candidate=@{kind='prior debug qualification executable';executable=$rollback;sha256=$RollbackSha256;scope='Same-format canonical read compatibility after production restore/rebind/encrypted publication; no downgrade write or format migration'}
    restore_configuration='Explicit disposable --sync-root supplies the required private trust exclusion; default fresh restore without an existing destination or known/declared sync root is not covered by passing rows'
    tools=@{git_sha256=(Get-FileHash -LiteralPath $git).Hash.ToLowerInvariant();age_sha256=(Get-FileHash -LiteralPath $age).Hash.ToLowerInvariant();node_sha256=(Get-FileHash -LiteralPath $node).Hash.ToLowerInvariant();verifier_sha256=(Get-FileHash -LiteralPath $verifier).Hash.ToLowerInvariant()}
    limitations=@('Current-host local restore only; machine handoff skipped', 'No physical full-volume exhaustion', 'No deterministic production crash-barrier injection', 'No model inference', 'Unknown third-party synchronization roots must be supplied explicitly through SyncRoots', 'Vault plaintext scan is of final published inventory, not a concurrent interrupted-write observer', 'Finite CLI authentication, path, and fresh encrypted-write checks; historical full crypto matrix retained separately')
}
function Save-Report { $report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8 }
function Assert-True([bool]$Value,[string]$Message) { if (-not $Value) { throw $Message } }
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Invoke-BoundedProcess([string]$Name,[Diagnostics.ProcessStartInfo]$ProcessInfo,[int]$Seconds,[long]$OutputLimit,[string]$InputText='') {
    $outPath=Join-Path $root ($Name+'.stdout.jsonl'); $errPath=Join-Path $root ($Name+'.stderr.log')
    $outFile=[IO.File]::Create($outPath); $errFile=[IO.File]::Create($errPath)
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$ProcessInfo
    $cancellation=[Threading.CancellationTokenSource]::new()
    $watch=[Diagnostics.Stopwatch]::StartNew()
    $started=$false; $reaped=$false; $timedOut=$false; $outputBound=$false
    $drainTimedOut=$false; $drainError=$null; $supervisionError=$null; $cleanupComplete=$true
    $killRequested=$false; $code=$null; $outCopy=$null; $errCopy=$null; $inputWrite=$null
    try {
        $started=$process.Start(); if (-not $started) { throw 'Process start failed' }
        $outCopy=$process.StandardOutput.BaseStream.CopyToAsync($outFile,81920,$cancellation.Token)
        $errCopy=$process.StandardError.BaseStream.CopyToAsync($errFile,81920,$cancellation.Token)
        if ($InputText) {
            $inputBytes=[Text.Encoding]::UTF8.GetBytes($InputText)
            $inputWrite=$process.StandardInput.BaseStream.WriteAsync($inputBytes,0,$inputBytes.Length,$cancellation.Token)
            $null=$inputWrite.WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
        }
        $process.StandardInput.BaseStream.Dispose()
        while (-not $process.WaitForExit(100)) {
            $outputBound=($outFile.Length -gt $OutputLimit -or $errFile.Length -gt 64KB)
            if ($watch.Elapsed.TotalSeconds -gt $Seconds -or $outputBound) {
                $timedOut= -not $outputBound; $killRequested=$true; $process.Kill($true)
                $reaped=$process.WaitForExit(10000)
                if (-not $reaped) { throw 'Process did not exit within the 10-second reap deadline' }
                break
            }
        }
        $reaped=$process.HasExited
        if ($reaped) { $code=$process.ExitCode }
        try {
            $null=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($outCopy,$errCopy)).WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
        } catch [TimeoutException] { $drainTimedOut=$true; $drainError='Output pipes exceeded the 10-second drain deadline' }
        catch { $drainError=$_.Exception.Message }
        $outputBound=$outputBound -or $outFile.Length -gt $OutputLimit -or $errFile.Length -gt 64KB
    } catch { $supervisionError=$_.Exception.Message }
    finally {
        if ($started) {
            try {
                if (-not $process.HasExited) { $killRequested=$true; $process.Kill($true); $reaped=$process.WaitForExit(10000) }
                else { $reaped=$true }
                if ($reaped) { $code=$process.ExitCode }
            } catch { $supervisionError=$_.Exception.Message; $reaped=$false }
        }
        $ioTasks=[Threading.Tasks.Task[]]@(@($outCopy,$errCopy,$inputWrite) | Where-Object { $null -ne $_ })
        if (@($ioTasks | Where-Object { -not $_.IsCompleted }).Count) {
            $cancellation.Cancel()
            if ($started) {
                $process.StandardOutput.Dispose(); $process.StandardError.Dispose()
                $process.StandardInput.BaseStream.Dispose()
            }
            try { $null=[Threading.Tasks.Task]::WhenAll($ioTasks).WaitAsync([TimeSpan]::FromSeconds(5)).GetAwaiter().GetResult() }
            catch { if (@($ioTasks | Where-Object { -not $_.IsCompleted }).Count) { $cleanupComplete=$false } }
        }
        foreach ($ioTask in $ioTasks) { if ($ioTask.IsFaulted) { $null=$ioTask.Exception } }
        $watch.Stop(); $outFile.Dispose(); $errFile.Dispose(); $cancellation.Dispose(); $process.Dispose()
    }
    $metrics=[ordered]@{exit_code=$code;elapsed_ms=$watch.ElapsedMilliseconds;timed_out=$timedOut;output_bound=$outputBound;process_reaped=$reaped;process_tree_kill_requested=$killRequested;descendant_reap_verified=$false;supervision_error=$supervisionError;drain_timed_out=$drainTimedOut;drain_error=$drainError;unresolved_pipe_holder=($drainTimedOut -and $reaped);pipe_cleanup_complete=$cleanupComplete;stdout_sha256=(Hash $outPath);stderr_sha256=(Hash $errPath)}
    $supervised=$started -and $reaped -and $cleanupComplete -and -not ($timedOut -or $outputBound -or $drainTimedOut -or $drainError -or $supervisionError)
    # Do not parse partial output or continue the campaign after unresolved
    # ownership. Parent exit alone cannot prove descendant termination.
    $stdout=if ($supervised) { [IO.File]::ReadAllText($outPath) } else { '' }
    $stderr=if ($supervised) { [IO.File]::ReadAllText($errPath) } else { '' }
    return @{metrics=$metrics;supervised=$supervised;stdout=$stdout;stderr=$stderr}
}
function Invoke-Helper([string]$Name,[string]$Program,[string]$WorkingDirectory,[string[]]$Arguments,[string]$InputText='') {
    $info=[Diagnostics.ProcessStartInfo]::new($Program)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.WorkingDirectory=$WorkingDirectory
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.RedirectStandardInput=$true
    $info.Environment.Clear()
    foreach ($environmentName in @('SystemRoot','WINDIR','PATH','TEMP','TMP')) { $value=[Environment]::GetEnvironmentVariable($environmentName); if ($null -ne $value) { $info.Environment[$environmentName]=$value } }
    $info.Environment['GIT_CONFIG_NOSYSTEM']='1'; $info.Environment['GIT_CONFIG_GLOBAL']='NUL'
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $execution=Invoke-BoundedProcess $Name $info 90 1MB $InputText
    $step=[ordered]@{name=$Name;program_sha256=(Hash $Program);arguments=$Arguments}
    foreach ($field in $execution.metrics.Keys) { $step[$field]=$execution.metrics[$field] }
    $report.steps += $step
    Save-Report
    Assert-True ($execution.supervised -and $execution.metrics.exit_code -eq 0) "$Name failed; inspect retained helper supervision receipt"
    return $execution.stdout
}
function Invoke-Candidate([string]$Name,[string]$Data,[string]$Workspace,[string[]]$Arguments,[bool]$Reject=$false,[string]$ExpectedDiagnostic='',[string]$CandidateExecutable=$exe) {
    $candidateHash=Hash $CandidateExecutable
    $candidateKind=if ($CandidateExecutable -ceq $exe -and $candidateHash -ceq $ExpectedSha256) { 'production' }
        elseif ($rollback -and $CandidateExecutable -ceq $rollback -and $candidateHash -ceq $RollbackSha256) { 'prior debug qualification executable' }
        else { throw 'Candidate path/digest differs from an explicitly frozen executable' }
    $info=[Diagnostics.ProcessStartInfo]::new($CandidateExecutable)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.RedirectStandardInput=$true
    $info.WorkingDirectory=$root; $info.Environment.Clear()
    foreach ($environmentName in @('SystemRoot','WINDIR','PATH','TEMP','TMP','USERPROFILE','LOCALAPPDATA')) {
        $value=[Environment]::GetEnvironmentVariable($environmentName); if ($null -ne $value) { $info.Environment[$environmentName]=$value }
    }
    $command=@('--format','jsonl','--non-interactive','--data-dir',$Data,'--workspace',$Workspace)+$Arguments
    foreach ($argument in $command) { $info.ArgumentList.Add($argument) }
    $execution=Invoke-BoundedProcess $Name $info $DeadlineSeconds 4MB
    $step=[ordered]@{name=$Name;executable=$CandidateExecutable;executable_sha256=$candidateHash;candidate_kind=$candidateKind;arguments=$command;expected_rejection=$Reject;expected_diagnostic=$ExpectedDiagnostic}
    foreach ($field in $execution.metrics.Keys) { $step[$field]=$execution.metrics[$field] }
    $report.steps += $step
    Save-Report
    Assert-True $execution.supervised "$Name process/output supervision failed; inspect retained receipt"
    $stdout=$execution.stdout; $stderr=$execution.stderr; $code=$execution.metrics.exit_code
    if ($Reject) {
        Assert-True ($code -ne 0) "$Name unexpectedly accepted"
        if ($ExpectedDiagnostic) { Assert-True (($stdout+$stderr).Contains($ExpectedDiagnostic)) "$Name failed for unexpected reason" }
        return $null
    }
    Assert-True ($code -eq 0) "$Name rejected; inspect retained stderr"
    $rows=@($stdout -split '\r?\n' | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json })
    $results=@($rows | Where-Object { $_.type -eq 'result' -and $null -ne $_.data })
    Assert-True ($results.Count -eq 1) "$Name requires exactly one structured result"
    return $results[0].data
}
try {
    if ($SyntheticProfile) {
        $guardRoot=Join-Path $root 'production-schema'; $guardData=Join-Path $guardRoot 'data'; $guardWorkspace=Join-Path $guardRoot 'workspace'
        foreach ($directory in @($guardRoot,$guardData,$guardWorkspace)) { New-Item -ItemType Directory -Path $directory | Out-Null }
        $profile=Get-Content -LiteralPath $SyntheticProfile -Raw | ConvertFrom-Json -AsHashtable
        $profile.Remove('qualification_endpoint') | Out-Null
        # The missing profile workspace forces the schema-positive control to
        # stop before canonical state or provider admission, even with credentials.
        $profile.workspace=Join-Path $guardRoot 'deliberately-missing-profile-workspace'
        $profilePath=Join-Path $guardRoot 'profile.json'
        $profile | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $profilePath -Encoding utf8
        $null=Invoke-Candidate 'production-schema-positive-control' $guardData $guardWorkspace @('--config',$profilePath,'run','Do not dispatch; schema control') $true
        $controlText=[IO.File]::ReadAllText((Join-Path $root 'production-schema-positive-control.stderr.log'))+[IO.File]::ReadAllText((Join-Path $root 'production-schema-positive-control.stdout.jsonl'))
        Assert-True ($controlText.Contains('profile workspace unavailable')) 'Supplied synthetic profile did not pass production schema positive control'
        $profile.qualification_endpoint='http://127.0.0.1:9'
        $profile | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $profilePath -Encoding utf8
        $null=Invoke-Candidate 'production-qualification-endpoint-denied' $guardData $guardWorkspace @('--config',$profilePath,'run','Do not dispatch; schema guard') $true
        $deniedText=[IO.File]::ReadAllText((Join-Path $root 'production-qualification-endpoint-denied.stderr.log'))+[IO.File]::ReadAllText((Join-Path $root 'production-qualification-endpoint-denied.stdout.jsonl'))
        Assert-True ($deniedText.Contains('invalid user profile or unknown setting')) 'Production accepted qualification endpoint schema'
        Assert-True (@(Get-ChildItem -LiteralPath $guardData -Recurse -File).Count -eq 0) 'Schema rejection admitted durable work'
        $report.production_qualification_endpoint_guard='pass'
    } else { $report.production_qualification_endpoint_guard='not_run: no complete synthetic profile supplied' }
    foreach ($backend in @('sqlite','files')) {
        $seed=Join-Path $fixtures $backend
        $fixturePath=Join-Path $seed 'fixture.json'; $fixture=Get-Content -LiteralPath $fixturePath -Raw | ConvertFrom-Json
        $source=Join-Path $seed 'snapshot.age'
        $keys=@(Get-ChildItem -LiteralPath (Join-Path $seed 'recovery') -Filter '*.recovery' -File)
        Assert-True ($keys.Count -eq 1) 'One retained recovery copy required'
        $key=$keys[0].FullName
        $other=if ($backend -eq 'sqlite') { 'files' } else { 'sqlite' }
        $wrongKeys=@(Get-ChildItem -LiteralPath (Join-Path $fixtures "$other/recovery") -Filter '*.recovery' -File)
        Assert-True ($wrongKeys.Count -eq 1) 'One independent wrong-key fixture required'
        $sourceHash=Hash $source; $fixtureHash=Hash $fixturePath; $keyHash=Hash $key
        Assert-True ($sourceHash -ceq $fixture.ciphertext_sha256) 'Retained ciphertext differs from fixture'
        $report.fixtures += @{backend=$backend;fixture_sha256=$fixtureHash;ciphertext_sha256=$sourceHash;recovery_copy_sha256=$keyHash}
        $case=Join-Path $root $backend; $data=Join-Path $case 'data'; $enrollment=Join-Path $case 'enrollment'; $stage=Join-Path $case 'staging'; $dest=Join-Path $case 'restored'
        $declaredSyncRoot=Join-Path $case 'declared-sync-exclusion'
        foreach ($directory in @($case,$data,$enrollment,$stage,$declaredSyncRoot)) { New-Item -ItemType Directory -Path $directory | Out-Null }
        $checkpoint=Join-Path $case 'checkpoint.json'
        [IO.File]::WriteAllText($checkpoint,($fixture.checkpoint | ConvertTo-Json -Compress))
        $null=Invoke-Candidate "$backend-enroll" $data $enrollment @('backup','keys','--workspace-id',$fixture.workspace,'import','--key',$key,'--lineage',$fixture.lineage,'--checkpoint',$checkpoint)
        # Fresh destination does not exist and sanitized child environments have
        # no ambient OneDrive roots. TrustStore requires a nonempty exclusion
        # set; declare an explicit disjoint disposable sync boundary for restore.
        $base=@('restore','--workspace-id',$fixture.workspace,'--source',$source,'--key',$key,'--staging',$stage,'--backend',$backend,'--sync-root',$declaredSyncRoot)
        $workspaceDigest=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($fixture.workspace))).ToLowerInvariant()
        $descriptor=Join-Path $data "workspaces/$workspaceDigest/workspace.json"
        foreach ($negative in @('wrong-key','missing-key','tampered','truncated')) {
            $argsCopy=$base.Clone()
            if ($negative -eq 'wrong-key') { $argsCopy[6]=$wrongKeys[0].FullName }
            elseif ($negative -eq 'missing-key') { $argsCopy[6]=Join-Path $case 'missing.recovery' }
            else {
                $bytes=[IO.File]::ReadAllBytes($source)
                if ($negative -eq 'tampered') { $bytes[$bytes.Length-1]=$bytes[$bytes.Length-1] -bxor 1 }
                else { $bytes=$bytes[0..($bytes.Length-33)] }
                $copy=Join-Path $case "$negative.age"; [IO.File]::WriteAllBytes($copy,$bytes); $argsCopy[4]=$copy
            }
            $diagnostic=switch ($negative) {
                'wrong-key' { 'recovery key is not independently enrolled' }
                'missing-key' { 'recovery material is unavailable or outside the permitted private directory' }
                default { 'ciphertext incomplete or authentication failed' }
            }
            $null=Invoke-Candidate "$backend-$negative" $data $dest ($argsCopy+@('--preview')) $true $diagnostic
            Assert-True (-not (Test-Path -LiteralPath $descriptor) -and -not (Test-Path -LiteralPath $dest)) "$negative changed active selection or materialized destination"
        }
        $preview=Invoke-Candidate "$backend-preview" $data $dest ($base+@('--preview'))
        Assert-True ($preview.preview -eq $true -and $preview.recovery_verified -eq $true -and $null -eq $preview.expected_descriptor -and $preview.ciphertext_sha256 -ceq $sourceHash) 'Fresh preview authentication/identity failed'
        $apply=$base+@('--operation',$preview.operation,'--ciphertext-sha256',$preview.ciphertext_sha256,'--bytes',([string]$preview.bytes))
        # Canonical data is forbidden plaintext staging. Authentication succeeds first;
        # apply must reject without activating or materializing the source tree.
        $unsafe=$apply.Clone(); $unsafe[8]=$data
        $null=Invoke-Candidate "$backend-unsafe-staging" $data $dest $unsafe $true 'workspace access denied'
        Assert-True (-not (Test-Path -LiteralPath $descriptor) -and -not (Test-Path -LiteralPath $dest)) 'Unsafe staging activated or materialized restore'
        # Use a fresh operation after the intentionally rejected staging attempt.
        $preview=Invoke-Candidate "$backend-preview-after-denial" $data $dest ($base+@('--preview'))
        $apply=$base+@('--operation',$preview.operation,'--ciphertext-sha256',$preview.ciphertext_sha256,'--bytes',([string]$preview.bytes))
        $restored=Invoke-Candidate "$backend-apply" $data $dest $apply
        Assert-True ($restored.activated -eq $true -and $restored.tasks_resumed -eq $false -and $restored.execution_grants_restored -eq $false -and $restored.rebind.trust -eq 'untrusted') 'Restore activation/trust contract failed'
        foreach ($file in $fixture.expected_files.PSObject.Properties) {
            $sourcePath=[IO.Path]::GetFullPath((Join-Path $dest $file.Name))
            Assert-True ($sourcePath.StartsWith(([IO.Path]::GetFullPath($dest)+[IO.Path]::DirectorySeparatorChar),[StringComparison]::OrdinalIgnoreCase)) 'Fixture path escapes restored workspace'
            $expectedBytes=[Text.Encoding]::UTF8.GetBytes([string]$file.Value)
            $expectedHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($expectedBytes)).ToLowerInvariant()
            Assert-True ((Hash $sourcePath) -ceq $expectedHash) 'Restored source bytes differ'
        }
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $dest '.git'))) 'Restore fabricated executable Git metadata'
        $selected=Get-Content -LiteralPath $descriptor -Raw | ConvertFrom-Json
        Assert-True ($selected.config.backend -eq $backend -and $selected.rebind_pending -eq $false) 'Backend/selection mismatch'
        foreach ($task in @($fixture.root_task,$fixture.child_task)) {
            $status=Invoke-Candidate "$backend-status-$task" $data $dest @('tasks','status',$task)
            Assert-True ($status.records.Count -gt 0 -and $status.records[0].state -eq 'paused') 'Restored task is not paused'
        }
        foreach ($view in @('chain','costs','policy')) { $null=Invoke-Candidate "$backend-inspect-$view" $data $dest @('inspect',$fixture.root_task,'--view',$view,'--limit','128') }
        $descriptorHash=Hash $descriptor
        $null=Invoke-Candidate "$backend-post-activation-wrong-key" $data $dest (@('restore','--workspace-id',$fixture.workspace,'--source',$source,'--key',$wrongKeys[0].FullName,'--staging',$stage,'--backend',$backend,'--preview')) $true 'recovery key is not independently enrolled'
        Assert-True ((Hash $descriptor) -ceq $descriptorHash) 'Rejected recovery changed active descriptor'
        # Only the fresh restored synthetic workspace receives Git metadata.
        # Explicitly reconcile its changed repository identity before publication.
        $hooks=Join-Path $case 'empty-git-hooks'; New-Item -ItemType Directory -Path $hooks | Out-Null
        $gitPrefix=@('-c',"core.hooksPath=$hooks",'-c','core.autocrlf=false','-c','commit.gpgsign=false')
        $null=Invoke-Helper "$backend-git-init" $git $dest ($gitPrefix+@('init','--quiet',"--template=$hooks"))
        $null=Invoke-Helper "$backend-git-add" $git $dest ($gitPrefix+@('add','--')+@($fixture.expected_files.PSObject.Properties.Name))
        $null=Invoke-Helper "$backend-git-commit" $git $dest ($gitPrefix+@('-c','user.name=VCP production fixture','-c','user.email=fixture@example.invalid','commit','--quiet','-m','Synthetic recovered checkpoint'))
        $rebound=Invoke-Candidate "$backend-rebind-git" $data $dest @('rebind',$fixture.workspace)
        # Restored source remains untrusted through rebind. Enroll only this
        # disposable synthetic workspace explicitly; preserve plan policy and
        # paused tasks instead of importing authority from the snapshot.
        $trusted=Invoke-Candidate "$backend-explicit-source-trust" $data $dest @('workspace','trust',$fixture.workspace,'--expected-revision',([string]$rebound.workspace_revision))
        Assert-True ($trusted.trust -eq 'trusted' -and $trusted.policy_mode -eq 'plan' -and $trusted.tasks_resumed -eq $false -and $trusted.provider_requests -eq 0 -and $trusted.execution_profiles_loaded -eq $false) 'Explicit synthetic source trust changed execution authority or dispatched work'
        $beforeKeys=Invoke-Candidate "$backend-write-key-before" $data $dest @('backup','keys','verify','--key',$key)
        $records=@()
        foreach ($task in @($fixture.root_task,$fixture.child_task)) {
            $status=Invoke-Candidate "$backend-write-baseline-$task" $data $dest @('tasks','status',$task)
            $records += @{collection='task';id=$task;value=$status.records[0]}
        }
        $costs=Invoke-Candidate "$backend-write-baseline-costs" $data $dest @('inspect',$fixture.root_task,'--view','costs','--limit','128')
        Assert-True ($null -eq $costs.next_cursor) 'Synthetic cost baseline exceeds one explicit page'
        foreach ($row in $costs.items) { if ($row.collection -eq 'ledger') { $records += @{collection='ledger';id=$row.id;value=$row.record} } }
        Assert-True (@($records | Where-Object collection -eq 'ledger').Count -gt 0) 'Retained fixture lacks ledger baseline'
        $vault=Join-Path $case 'fresh-vault'; $writeStage=Join-Path $case 'write-staging'
        foreach ($directory in @($vault,$writeStage)) { New-Item -ItemType Directory -Path $directory | Out-Null }
        $null=Invoke-Candidate "$backend-write-configure" $data $dest @('backup','configure','--vault',$vault,'--staging',$writeStage,'--manual-only')
        $operation=[guid]::NewGuid().ToString()
        $null=Invoke-Candidate "$backend-write-create" $data $dest @('backup','create','--key',$key,'--git',$git,'--operation',$operation)
        $afterKeys=Invoke-Candidate "$backend-write-key-after" $data $dest @('backup','keys','verify','--key',$key)
        Assert-True ($afterKeys.configuration.checkpoint.sequence -eq ($beforeKeys.configuration.checkpoint.sequence+1)) 'Publication checkpoint did not advance exactly once'
        $objects=@(Get-ChildItem -LiteralPath $vault -File -Recurse)
        Assert-True ($objects.Count -eq 1 -and $objects[0].Name -ceq "$operation.age") 'Vault contains unexpected publication files'
        $expected=@{workspace=$fixture.workspace;lineage=$beforeKeys.configuration.lineage;writer=$beforeKeys.configuration.selected.writer;sequence=$afterKeys.configuration.checkpoint.sequence;deletion=$afterKeys.configuration.checkpoint.deletion;parent=$beforeKeys.configuration.checkpoint.parent;manifest_sha256=$afterKeys.configuration.checkpoint.parent;files=$fixture.expected_files;records=$records}
        $independentInput=@{age=$age;key=$key;object=$objects[0].FullName;expected=$expected} | ConvertTo-Json -Depth 100 -Compress
        $verifiedText=Invoke-Helper "$backend-independent-write-verify" $node $case @($verifier) $independentInput
        $verified=$verifiedText | ConvertFrom-Json
        Assert-True ($verified.status -eq 'pass' -and $verified.signature_verified -eq $true -and $verified.source_files_verified -eq @($fixture.expected_files.PSObject.Properties).Count) 'Independent production snapshot validation failed'
        $rollbackResult=@{status='not_run';reason='No validated prior debug executable supplied'}
        if ($rollback) {
            # The independent snapshot verifier just proved the captured task
            # and ledger records survive production checkpoint/publication.
            # Read those same records with the prior debug artifact, then fresh
            # production processes: exactly six additional CLI invocations.
            $writtenDescriptorHash=Hash $descriptor
            $publishedCiphertextHash=Hash $objects[0].FullName
            $baselineLedger=@($records | Where-Object collection -eq 'ledger' | Sort-Object id | ForEach-Object { $_.value })
            $baselineLedgerJson=ConvertTo-Json -InputObject $baselineLedger -Depth 100 -Compress
            foreach ($reader in @(@{name='prior-debug-rollback';path=$rollback},@{name='production-after-rollback';path=$exe})) {
                foreach ($task in @($fixture.root_task,$fixture.child_task)) {
                    $observed=Invoke-Candidate -Name "$backend-$($reader.name)-$task" -Data $data -Workspace $dest -Arguments @('tasks','status',$task) -CandidateExecutable $reader.path
                    $baseline=@($records | Where-Object { $_.collection -eq 'task' -and $_.id -ceq $task })
                    Assert-True ($baseline.Count -eq 1 -and $observed.records.Count -eq 1) 'Rollback task identity/count differs'
                    $baselineJson=ConvertTo-Json -InputObject $baseline[0].value -Depth 100 -Compress
                    $observedJson=ConvertTo-Json -InputObject $observed.records[0] -Depth 100 -Compress
                    Assert-True ($observedJson -ceq $baselineJson) 'Rollback or production reopen changed retained task record'
                }
                $observedCosts=Invoke-Candidate -Name "$backend-$($reader.name)-costs" -Data $data -Workspace $dest -Arguments @('inspect',$fixture.root_task,'--view','costs','--limit','128') -CandidateExecutable $reader.path
                Assert-True ($null -eq $observedCosts.next_cursor) 'Rollback ledger baseline exceeds explicit page'
                $observedLedger=@($observedCosts.items | Where-Object collection -eq 'ledger' | Sort-Object id | ForEach-Object { $_.record })
                $observedLedgerJson=ConvertTo-Json -InputObject $observedLedger -Depth 100 -Compress
                Assert-True ($observedLedgerJson -ceq $baselineLedgerJson) 'Rollback or production reopen changed retained ledger records'
                Assert-True ((Hash $descriptor) -ceq $writtenDescriptorHash) 'Rollback or production reopen changed selected descriptor'
                Assert-True ((Hash $objects[0].FullName) -ceq $publishedCiphertextHash) 'Rollback or production reopen changed published snapshot'
            }
            Assert-True ((Hash $rollback) -ceq $RollbackSha256) 'Prior debug executable changed during rollback checks'
            $rollbackResult=@{status='pass';prior_candidate_kind='debug qualification';prior_executable_sha256=$RollbackSha256;production_executable_sha256=$ExpectedSha256;task_records_compared=2;ledger_records_compared=$baselineLedger.Count;descriptor_sha256=$writtenDescriptorHash;published_ciphertext_sha256=$publishedCiphertextHash;production_fresh_reopen=$true;scope='same-format canonical task/ledger read compatibility'}
        }
        Assert-True ((Hash $source) -ceq $sourceHash -and (Hash $fixturePath) -ceq $fixtureHash -and (Hash $key) -ceq $keyHash) 'Original fixture changed'
        $report.backends += @{backend=$backend;status='pass';restored_descriptor_sha256=$descriptorHash;final_descriptor_sha256=(Hash $descriptor);source_files_verified=$fixture.expected_files.PSObject.Properties.Name;root_task=$fixture.root_task;child_task=$fixture.child_task;fresh_encryption=$verified;canonical_rollback=$rollbackResult}
        Save-Report
    }
    Assert-True ((Hash $exe) -ceq $ExpectedSha256) 'Candidate changed during qualification'
    $report.status='pass'
} catch { $report.status='fail'; $report.failure=$_.Exception.Message; throw }
finally { $report.completed_at=[DateTime]::UtcNow.ToString('o'); Save-Report }
