# SPDX-License-Identifier: Apache-2.0
# Native synthetic denial controls using the existing zero-capability fixture.
#requires -Version 7.0
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$output=[IO.Path]::GetFullPath($OutputDirectory)
if (-not $output.StartsWith((Join-Path $repository 'artifacts')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)) { throw 'New repository artifact directory required' }
$ancestor=Split-Path -Parent $output
while($ancestor){
  if(Test-Path -LiteralPath $ancestor){
    $entry=Get-Item -LiteralPath $ancestor -Force
    if(-not $entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Artifact ancestor must be an ordinary directory'}
  }
  if($ancestor -eq $repository){break}
  $ancestor=Split-Path -Parent $ancestor
}
New-Item -ItemType Directory -Path $output | Out-Null
$node=(Get-Command node.exe -ErrorAction Stop).Source
$canary=Join-Path $PSScriptRoot 'cs3-boundary-canary.cjs'
$fixtureSource=Join-Path $repository 'src/tests/support/windows/AppContainerFixture.cs'
$breakawaySource=Join-Path $repository 'src/tests/support/windows/Cs3BreakawayCanary.cs'
$breakawayBinary=Join-Path $output 'Cs3BreakawayCanary.exe'
& 'C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe' /nologo /platform:x64 /optimize+ /target:exe "/out:$breakawayBinary" $breakawaySource
if($LASTEXITCODE -ne 0){throw 'Breakaway canary compilation failed'}
function Hash([string]$File) { (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant() }
$record=[ordered]@{schema='cs3-native-denial-qualification/1';status='running';model_calls=0;node_sha256=(Hash $node);canary_sha256=(Hash $canary);fixture_sha256=(Hash $fixtureSource);runner_sha256=(Hash $PSCommandPath);runs=@();limitations=@('Synthetic native token canaries; browser descendant token/image/job identity is joined separately.','Installed Windows/runtime readable package resources remain required runtime access, not a hidden filesystem.');cleanup='pending'}
$record.breakaway_source_sha256=Hash $breakawaySource
$record.breakaway_binary_sha256=Hash $breakawayBinary
$record.profiles=@()
function Save { [IO.File]::WriteAllText((Join-Path $output 'result.json'),($record|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false)) }
Add-Type -Path $fixtureSource
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User
function Acl([string]$Directory,[string]$Extra,[string]$Rights) {
  $acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetAccessRuleProtection($true,$false)
  foreach($sid in @($owner,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) { $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow')) }
  if($Extra){$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($Extra),$Rights,'ContainerInherit,ObjectInherit','None','Allow'))}
  Set-Acl -LiteralPath $Directory -AclObject $acl
  & icacls $Directory /setintegritylevel '(OI)(CI)L' | Out-Null
  if($LASTEXITCODE -ne 0){throw 'Disposable canary integrity label failed'}
}
$v4=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
$v6=[Net.Sockets.TcpListener]::new([Net.IPAddress]::IPv6Loopback,0)
try {
  $v4.Start(8);$v6.Start(8)
  foreach($mode in @('control','restricted','control')) {
    $profileName='iokaio.vcp.memory.'+[guid]::NewGuid().ToString('N')
    $profileRecord=[ordered]@{name=$profileName;mode=$mode;state='reserved';sid=$null;root=Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) ('Packages/'+$profileName+'/AC')}
    $record.profiles+=$profileRecord
    Save
    $fixture=[Vcp.Qualification.AppContainerFixture]::new($profileName);$junction=$null;$rowRecord=$null
    try {
      $profileRecord.state='created';$profileRecord.sid=$fixture.Sid
      Save
      $root=$fixture.Root
      Acl $root $fixture.Sid 'Modify'
      $outside=Join-Path $output ('canary-'+[guid]::NewGuid().ToString('N'))
      New-Item -ItemType Directory -Path $outside | Out-Null
      $files=@();$groups=@('','S-1-1-0','S-1-5-32-545','S-1-5-11')
      for($i=0;$i -lt 4;$i++) {
        $dir=Join-Path $outside "g$i";New-Item -ItemType Directory -Path $dir | Out-Null
        Acl $dir $groups[$i] 'Modify'
        $file=Join-Path $dir 'owner.txt';[IO.File]::WriteAllText($file,"CS3_PUBLIC_CANARY`n",[Text.UTF8Encoding]::new($false));$files+=$file
      }
      $inside=Join-Path $root 'work';New-Item -ItemType Directory -Path $inside | Out-Null;Acl $inside $fixture.Sid 'Modify'
      [IO.File]::WriteAllText((Join-Path $inside 'allowed.txt'),"CS3_PUBLIC_CANARY`n",[Text.UTF8Encoding]::new($false))
      $readonly=Join-Path $root 'readonly';New-Item -ItemType Directory -Path $readonly | Out-Null;Acl $readonly $fixture.Sid 'ReadAndExecute'
      $readfile=Join-Path $readonly 'resource.txt';[IO.File]::WriteAllText($readfile,"CS3_PUBLIC_CANARY`n",[Text.UTF8Encoding]::new($false))
      $junction=Join-Path $inside 'redirect';New-Item -ItemType Junction -Path $junction -Target (Split-Path -Parent $files[0]) | Out-Null
      $exe=Join-Path $root 'node.exe';Copy-Item -LiteralPath $node -Destination $exe
      & icacls $exe /setintegritylevel M | Out-Null
      if($LASTEXITCODE -ne 0){throw 'Disposable executable label failed'}
      $code=Join-Path $root 'probe.cjs';Copy-Item -LiteralPath $canary -Destination $code
      if((Hash $exe) -cne $record.node_sha256 -or (Hash $code) -cne $record.canary_sha256){throw 'Staged probe differs'}
      $input=[ordered]@{mode=$mode;inside=$inside;files=$files;readonly=$readfile;ipv4=$v4.LocalEndpoint.Port;ipv6=$v6.LocalEndpoint.Port}
      $result=$fixture.RunBounded($exe,@('--no-addons','-e',[IO.File]::ReadAllText($code)),($mode -eq 'restricted'),[Text.Encoding]::UTF8.GetBytes(($input|ConvertTo-Json -Compress)),8192,268435456,15000,[Threading.CancellationToken]::None)
      if($result.Process.ExitCode -ne 0 -or $result.Termination -cne 'exited' -or $result.ErrorBytes -ne 0){
        $record.failed_process=[ordered]@{mode=$mode;exit_code=$result.Process.ExitCode;termination=$result.Termination;stderr=[Text.Encoding]::UTF8.GetString($result.Error)}
        throw 'Canary process failed'
      }
      $observations=[Text.Encoding]::UTF8.GetString($result.Output)|ConvertFrom-Json
      $record.last_observation=[ordered]@{mode=$mode;token=$result.Process;rows=$observations.rows}
      if($observations.schema -cne 'cs3-native-denial-canary/1' -or $observations.rows.Count -ne $(if($mode -eq 'restricted'){18}else{16})){throw 'Canary observation shape differs'}
      $restricted=$mode -eq 'restricted'
      if($result.Process.AppContainer -ne $restricted -or $result.Process.TokenSidMatchesProfile -ne $restricted -or $result.Process.CapabilityCount -ne 0){throw 'Canary token differs'}
      $expectedIds=@('owned_read','owned_write','host_read_0','host_write_0','host_read_1','host_write_1','host_read_2','host_write_2','host_read_3','host_write_3','junction_read','junction_write','staged_read','staged_write','ipv4_loopback','ipv6_loopback')
      if($restricted){$expectedIds+=@('external_ipv4','external_ipv6')}
      if(($observations.rows.id -join '|') -cne ($expectedIds -join '|')){throw 'Canary operation inventory differs'}
      foreach($row in $observations.rows){
        $allow=$mode -eq 'control' -or $row.id -in @('owned_read','owned_write','staged_read')
        if($allow){if($row.outcome -cne 'allowed'){throw "Positive control failed: $($row.id) $($row.code)"}}
        else {
          # Windows reports AppContainer loopback drops as a native timeout.
          # Require a socket error before our own watchdog, plus independent
          # listener absence bracketed by unrestricted successful connections.
          # External destinations still require explicit access denial.
          $loopbackDrop=$row.id -in @('ipv4_loopback','ipv6_loopback') -and $row.code -ceq 'ETIMEDOUT' -and $row.elapsed_ms -lt 1500
          if($row.outcome -cne 'denied' -or ($row.code -cnotin @('EACCES','EPERM') -and -not $loopbackDrop)){throw "Explicit denial absent: $($row.id) $($row.outcome) $($row.code)"}
        }
      }
      foreach($listener in @($v4,$v6)){
        if($mode -eq 'restricted'){if($listener.Pending()){throw 'Restricted probe reached listener'}}
        else{if(-not $listener.Pending()){throw 'Control failed to reach listener'};$client=$listener.AcceptTcpClient();$client.Dispose()}
      }
      foreach($file in $files+@((Join-Path $inside 'allowed.txt'),$readfile)){if([IO.File]::ReadAllText($file) -cne "CS3_PUBLIC_CANARY`n"){throw 'Synthetic file changed'}}
      $rowRecord=[ordered]@{mode=$mode;token=$result.Process;rows=$observations.rows;listeners_verified=$true;canaries_unchanged=$true;profile_removed=$false}
      $escape=Join-Path $root 'Cs3BreakawayCanary.exe';Copy-Item -LiteralPath $breakawayBinary -Destination $escape
      & icacls $escape /setintegritylevel M | Out-Null
      if($LASTEXITCODE -ne 0 -or (Hash $escape) -cne $record.breakaway_binary_sha256){throw 'Breakaway canary staging failed'}
      $childResult=$fixture.RunBounded($escape,[string[]]@(),($mode -eq 'restricted'),[byte[]]@(),2048,268435456,10000,[Threading.CancellationToken]::None)
      $record.breakaway_last=[ordered]@{exit_code=$childResult.Process.ExitCode;stderr=[Text.Encoding]::UTF8.GetString($childResult.Error);stdout=[Text.Encoding]::UTF8.GetString($childResult.Output)}
      if($childResult.Process.ExitCode -ne 0 -or $childResult.Termination -cne 'exited' -or $childResult.ErrorBytes -ne 0){throw 'No-breakaway control failed'}
      if($childResult.Process.AppContainer -ne $restricted -or $childResult.Process.TokenSidMatchesProfile -ne $restricted -or $childResult.Process.CapabilityCount -ne 0){throw 'Breakaway token differs'}
      $rowRecord.breakaway=[Text.Encoding]::UTF8.GetString($childResult.Output)|ConvertFrom-Json
      if($rowRecord.breakaway.schema -cne 'cs3-breakaway-canary/1' -or $rowRecord.breakaway.created -ne $false -or $rowRecord.breakaway.win32_error -ne 5){throw 'Breakaway outcome differs'}
      $record.runs+=$rowRecord
    } finally {
      if($junction -and (Test-Path -LiteralPath $junction)){
        $item=Get-Item -LiteralPath $junction -Force
        if(-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or -not $item.FullName.StartsWith($fixture.Root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Owned junction changed'}
        [IO.Directory]::Delete($junction)
      }
      $fixture.Dispose()
      if($rowRecord){$rowRecord.profile_removed=-not (Test-Path -LiteralPath $root)}
      $profileRecord.state='removed'
      Save
    }
  }
  if($record.runs.Count -ne 3 -or @($record.runs|Where-Object {-not $_.profile_removed}).Count){throw 'Incomplete canary run cleanup'}
  if((Hash $node) -cne $record.node_sha256 -or (Hash $canary) -cne $record.canary_sha256 -or (Hash $fixtureSource) -cne $record.fixture_sha256 -or (Hash $PSCommandPath) -cne $record.runner_sha256 -or (Hash $breakawaySource) -cne $record.breakaway_source_sha256 -or (Hash $breakawayBinary) -cne $record.breakaway_binary_sha256){throw 'Qualification inputs changed'}
  $record.inputs_unchanged=$true
  $record.status='pass'
} catch {$record.status='fail';$record.error=$_.Exception.Message;throw}
finally {$v4.Stop();$v6.Stop();$record.cleanup='listeners_closed';Save}
Write-Output (Join-Path $output 'result.json')
