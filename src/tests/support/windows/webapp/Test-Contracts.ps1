# SPDX-License-Identifier: Apache-2.0
# Pure bookkeeping only: no registry/ACL access, processes, Core or browser.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Controller-helpers.ps1')
. (Join-Path $PSScriptRoot 'Input-Policy.ps1')
. (Join-Path $PSScriptRoot 'Pe-Contract.ps1')
$script:checks=0
function Check([bool]$Value) { if (-not $Value) { throw 'Pure PowerShell assertion failed' }; $script:checks++ }
function Reject([scriptblock]$Action) { $failed=$false; try { & $Action } catch { $failed=$true }; Check $failed }
# Parse the real controller without invoking it. Pin the live protocol ordering:
# ownership is checkpointed, ordinary stream events do not rewrite the receipt,
# and independent job-zero observation precedes ACK without intervening disk I/O.
$tokens=$null;$parseErrors=$null
$controller=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'Invoke-NativeProbe.ps1'),[ref]$tokens,[ref]$parseErrors)
Check ($parseErrors.Count -eq 0)
$eventLoop=$controller.Find({param($node) $node -is [Management.Automation.Language.WhileStatementAst] -and $node.Condition.Extent.Text.Contains('$worker.HasExited')},$true)
$writes=@($eventLoop.Body.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -ceq 'JsonWrite'},$true))
Check ($writes.Count -eq 2)
$ownership=@($eventLoop.Body.Statements|Where-Object {$_ -is [Management.Automation.Language.IfStatementAst] -and $_.Extent.Text.Contains('Register-OwnedProcess $owned $event')})
Check ($ownership.Count -eq 1 -and $ownership[0].Extent.Text.Contains('JsonWrite $receiptPath $value'))
$ack=@($eventLoop.Body.Statements|Where-Object {$_ -is [Management.Automation.Language.IfStatementAst] -and $_.Extent.Text.Contains('Duplicate job drain handshake')})
$ackText=$ack[0].Extent.Text
$zeroAt=$ackText.IndexOf('::WaitForEmpty($port)');$sendAt=$ackText.IndexOf("WriteLine('DRAINED')")
Check ($ack.Count -eq 1 -and -not $ackText.Contains('JsonWrite') -and $zeroAt -ge 0 -and $sendAt -gt $zeroAt -and $ackText.Contains("if (-not `$independentEmpty) { throw"))
$supervisor=[IO.File]::ReadAllText((Join-Path $PSScriptRoot 'WebViewSupervisor.cs'))
$teardown=$supervisor.Substring($supervisor.IndexOf('// Stop the live PID collector before deliberate job teardown.'))
Check ($teardown.Contains('DrainCollectorAndJob(()=>{if(collector!=null)collector.Stop();},collect,()=> {'))
Check (-not $teardown.Contains('JobPids(') -and $teardown.Contains('Accounts(job).Active!=0'))
Check ($teardown.IndexOf('TerminateJobObject(job,1)') -lt $teardown.IndexOf('ProbeContract.Coverage(counts.Total,observed.Count)'))
Check ($teardown.IndexOf('ProbeContract.Coverage(counts.Total,observed.Count)') -lt $teardown.IndexOf('WaitForDrainAcknowledgement(owner)'))
$recorded=[pscustomobject]@{controller_pid=42;controller_creation_filetime=100L}
$identityState=@{disposed=0;lookups=0}
$fakeProcess=[pscustomobject]@{StartTime=[DateTime]::FromFileTimeUtc(100)}
$fakeProcess|Add-Member ScriptMethod Dispose {$identityState.disposed++}
Check (Test-ExactControllerAlive $recorded {$identityState.lookups++;$fakeProcess})
Check ($identityState.disposed -eq 1 -and $identityState.lookups -eq 1)
$fakeProcess.StartTime=[DateTime]::FromFileTimeUtc(101)
Check (-not (Test-ExactControllerAlive $recorded {$fakeProcess}))
Check ($identityState.disposed -eq 2)
Check (-not (Test-ExactControllerAlive $recorded {throw [ArgumentException]::new('missing PID')}))
# Exercise the same exception wrapping as a failing PowerShell static method.
Check (-not (Test-ExactControllerAlive $recorded {throw [Management.Automation.MethodInvocationException]::new('wrapped missing PID',[ArgumentException]::new('missing PID'))}))
$wrapped=$null
try {[ArgumentException]::ThrowIfNullOrEmpty('')} catch {$wrapped=$_.Exception}
Check ($wrapped -is [Management.Automation.MethodInvocationException] -and $wrapped.InnerException.GetType() -eq [ArgumentException])
Check (-not (Test-ExactControllerAlive $recorded {throw $wrapped}))
foreach($failure in @([ComponentModel.Win32Exception]::new(5),[IO.IOException]::new('query failed'),[ArgumentOutOfRangeException]::new('ProcessId'))) {
    Reject {Test-ExactControllerAlive $recorded {throw $failure}}
}
Reject {Test-ExactControllerAlive $recorded {throw [Management.Automation.MethodInvocationException]::new('wrapped denial',[ComponentModel.Win32Exception]::new(5))}}
Reject {Test-ExactControllerAlive $recorded {throw [IO.IOException]::new('not a PowerShell wrapper',[ArgumentException]::new('nested argument failure'))}}
Reject {Test-ExactControllerAlive $recorded {throw [InvalidOperationException]::new('unknown wrapper',[ArgumentException]::new('nested argument failure'))}}
Reject {Test-ExactControllerAlive $recorded {throw [Management.Automation.RuntimeException]::new('not method invocation',[ArgumentException]::new('nested argument failure'))}}
Reject {Test-ExactControllerAlive $recorded {throw [Management.Automation.MethodInvocationException]::new('method wrapper',[IO.IOException]::new('unknown nested wrapper',[ArgumentException]::new('nested argument failure')))}}
Reject {Test-ExactControllerAlive $recorded {$null}}
foreach($bad in @($null,0,-1,'42',42.5,2147483648L)) {
    $identityState.lookups=0
    Reject {Test-ExactControllerAlive ([pscustomobject]@{controller_pid=$bad;controller_creation_filetime=100L}) {$identityState.lookups++;throw 'Malformed identity reached lookup'}}
    Check ($identityState.lookups -eq 0)
}
foreach($bad in @($null,0L,-1L,'100',100.5,[long]::MaxValue)) {
    $identityState.lookups=0
    Reject {Test-ExactControllerAlive ([pscustomobject]@{controller_pid=42;controller_creation_filetime=$bad}) {$identityState.lookups++;throw 'Malformed identity reached lookup'}}
    Check ($identityState.lookups -eq 0)
}
foreach($failure in @([ComponentModel.Win32Exception]::new(5),[InvalidOperationException]::new('process exited during creation query'))) {
    $broken=[pscustomobject]@{}
    $broken|Add-Member ScriptProperty StartTime {throw $failure}
    $broken|Add-Member ScriptMethod Dispose {$identityState.disposed++}
    $before=$identityState.disposed
    Reject {Test-ExactControllerAlive $recorded {$broken}}
    Check ($identityState.disposed -eq $before+1)
}
# Execute the real recovery function's unknown-owner branch with only its three
# OS/file boundaries replaced. No profile, process or receipt operation occurs.
$recovery=$controller.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq 'Recover-AbandonedProfile'},$true)
$alive=(Get-Command Test-ExactControllerAlive).ScriptBlock
& {
    function ReadReceipt {param($File) $value}
    function Test-ExactControllerAlive {param($Value) & $alive $Value $lookup}
    function Test-DeleteProfile {$state.deleted++;$state.rootExists=$false}
    function Test-RegularTree {$state.treeChecks++}
    function Test-Path {param($LiteralPath) if($LiteralPath -ceq $value.root){return $state.rootExists}; return $true}
    function JsonWrite {$state.written++}
    $source=$recovery.Extent.Text.Replace('[Vcp.Cs3Draft.NativeProbe]::DeleteProfile($value.name, $value.sid)','Test-DeleteProfile')
    $source=$source.Replace('[Vcp.Cs3Draft.NativeProbe]::RegularTree($parent)','Test-RegularTree')
    . ([scriptblock]::Create($source))
    $fakeProcess.StartTime=[DateTime]::FromFileTimeUtc(100)
    $reusedProcess=[pscustomobject]@{StartTime=[DateTime]::FromFileTimeUtc(101)}
    $reusedProcess|Add-Member ScriptMethod Dispose {$identityState.disposed++}
    foreach($case in @(
        @{allow=$false;lookup={throw [ComponentModel.Win32Exception]::new(5)}},
        @{allow=$false;lookup={$broken}}, @{allow=$false;lookup={$fakeProcess}},
        @{allow=$true;lookup={throw [ArgumentException]::new('missing PID')}},
        @{allow=$true;lookup={$reusedProcess}}
    )) {
        $state=@{deleted=0;written=0;treeChecks=0;rootExists=$true};$lookup=$case.lookup
        $value=[pscustomobject]@{profile_created=$true;status='running';controller_pid=42;controller_creation_filetime=100L;processes_drained=$false;
            root=[IO.Path]::Combine([IO.Path]::GetTempPath(),'cs3-synthetic-owned','AC');name='iokaio.vcp.cs3.00000000000000000000000000000000';sid='S-1-15-2-1';outcome='not_run'}
        if($case.allow) {
            Check (Recover-AbandonedProfile 'synthetic-receipt')
            Check ($state.deleted -eq 1 -and $state.written -eq 1 -and $state.treeChecks -eq 1 -and $value.processes_drained -and $value.status -ceq 'owner_loss_recovered')
        } else {
            Reject {Recover-AbandonedProfile 'synthetic-receipt'}
            Check ($state.deleted -eq 0 -and $state.written -eq 0 -and $state.treeChecks -eq 0 -and -not $value.processes_drained -and $value.status -ceq 'running')
        }
    }
}
Assert-NoPolicyValueNames @('another-app.exe'); Check $true
foreach ($name in @('iokaio.vcp.cs3.webview2.probe','webviewhost.EXE','*')) { Reject { Assert-NoPolicyValueNames @($name) } }
Assert-NoWritableRuntimeRule 'S-1-15-2-1' ([int][Security.AccessControl.FileSystemRights]::ReadAndExecute) 'Allow'; Check $true
Assert-NoWritableRuntimeRule 'S-1-5-18' ([int][Security.AccessControl.FileSystemRights]::FullControl) 'Allow'; Check $true
foreach ($principal in @('S-1-1-0','S-1-5-11','S-1-5-32-545','S-1-15-2-1','S-1-15-2-2','S-1-15-2-999')) { Reject { Assert-NoWritableRuntimeRule $principal ([int][Security.AccessControl.FileSystemRights]::Write) 'Allow' 'S-1-15-2-999' } }
$held=[pscustomobject]@{tag='same held object'}; $identities=@{42=@{Process=$held;CreationFileTime=100}}
$actual=Register-OwnedProcess $identities ([pscustomobject]@{pid=42;creation_filetime=100}) { throw 'Must not reopen an existing held PID' }; Check ([object]::ReferenceEquals($actual,$held))
Reject { Register-OwnedProcess $identities ([pscustomobject]@{pid=42;creation_filetime=101}) { throw 'Must not reopen reused PID' } }
Reject { Register-OwnedProcess @{} ([pscustomobject]@{pid=43;creation_filetime=100}) { throw 'Uninspectable process must fail' } }
$short=@{};$shortResult=Register-OwnedProcess $short ([pscustomobject]@{pid=44;creation_filetime=101;token_verified=$true}) { throw [ArgumentException]::new('already exited') };Check ($null -eq $shortResult -and $short[44].ExitedBeforeParentOpen -and $short[44].CreationFileTime -eq 101)
Reject { Register-OwnedProcess @{} ([pscustomobject]@{pid=45;creation_filetime=102;token_verified=$false}) { throw [ArgumentException]::new('already exited') } }
$state=@{moves=0;validations=0;delays=0}
Complete-OwnedReceiptReplacement -Validate {$state.validations++} -Move {$state.moves++;if($state.moves -eq 1){throw [UnauthorizedAccessException]::new('synthetic transient access conflict')}} -Delay {$state.delays++}
Check ($state.moves -eq 2 -and $state.validations -eq 2 -and $state.delays -eq 1)
$state=@{moves=0;validations=0;delays=0}
Reject {Complete-OwnedReceiptReplacement -Validate {$state.validations++} -Move {$state.moves++;throw [IO.IOException]::new('synthetic sharing conflict',-2147024864)} -Delay {$state.delays++}}
Check ($state.moves -eq 10 -and $state.validations -eq 10 -and $state.delays -eq 9)
$state=@{moves=0;validations=0;delays=0}
Reject {Complete-OwnedReceiptReplacement -Validate {$state.validations++;if($state.validations -gt 1){throw 'synthetic changed receipt'}} -Move {$state.moves++;throw [UnauthorizedAccessException]::new('synthetic access conflict')} -Delay {$state.delays++}}
Check ($state.moves -eq 1 -and $state.validations -eq 2 -and $state.delays -eq 1)
$state=@{moves=0;delays=0}
Reject {Complete-OwnedReceiptReplacement -Validate {} -Move {$state.moves++;throw [IO.IOException]::new('synthetic unrelated failure',-2147024894)} -Delay {$state.delays++}}
Check ($state.moves -eq 1 -and $state.delays -eq 0)
Assert-SnapshotSame ([ordered]@{root='synthetic';files=@(@{sha256='a'})}) ([ordered]@{root='synthetic';files=@(@{sha256='a'})}); Check $true
Reject { Assert-SnapshotSame (@{sha256='a'}) (@{sha256='b'}) }
$pe=[byte[]]::new(256)
[BitConverter]::GetBytes([uint16]0x5a4d).CopyTo($pe,0)
[BitConverter]::GetBytes([uint32]64).CopyTo($pe,0x3c)
[BitConverter]::GetBytes([uint32]0x4550).CopyTo($pe,64)
[BitConverter]::GetBytes([uint16]0x8664).CopyTo($pe,68)
[BitConverter]::GetBytes([uint16]112).CopyTo($pe,84)
[BitConverter]::GetBytes([uint16]2).CopyTo($pe,86)
[BitConverter]::GetBytes([uint16]0x20b).CopyTo($pe,88)
[BitConverter]::GetBytes([uint16]2).CopyTo($pe,156)
Assert-PeExecutable $pe; Check $true
foreach($mutation in @(@{offset=0;value=0},@{offset=64;value=0},@{offset=68;value=0x14c},@{offset=84;value=70},@{offset=84;value=512},@{offset=86;value=0},@{offset=86;value=0x2002},@{offset=88;value=0x10b},@{offset=156;value=3})) {
    $changed=[byte[]]$pe.Clone(); [BitConverter]::GetBytes([uint16]$mutation.value).CopyTo($changed,$mutation.offset)
    Reject { Assert-PeExecutable $changed }
}
foreach($offset in @([uint32]0,[uint32]250,[uint32]::MaxValue)) {
    $changed=[byte[]]$pe.Clone(); [BitConverter]::GetBytes($offset).CopyTo($changed,0x3c)
    Reject { Assert-PeExecutable $changed }
}
Reject { Assert-PeExecutable ([byte[]]::new(63)) }
Reject { Assert-PeExecutable ([byte[]]$pe[0..100]) }
$console=[byte[]]$pe.Clone(); [BitConverter]::GetBytes([uint16]3).CopyTo($console,156)
Assert-PeExecutable $console 3; Check $true
'PASS '+$script:checks+' pure policy/ACL-rule/snapshot/held-identity/PE assertions; no OS policy or process operations.'
