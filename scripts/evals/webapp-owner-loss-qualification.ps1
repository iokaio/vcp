# SPDX-License-Identifier: Apache-2.0
# Kill only the exact newly owned controller, then invoke its source-bound recovery.
#requires -Version 7.0
param([Parameter(Mandatory)][string]$Build,[Parameter(Mandatory)][string]$ExpectedInputsSha256)
$ErrorActionPreference='Stop'
$buildRoot=[IO.Path]::GetFullPath($Build)
$allowed=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../artifacts'))+[IO.Path]::DirectorySeparatorChar
if(-not $buildRoot.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)){throw 'Exact repository artifact build required'}
$manifest=Join-Path $buildRoot 'inputs.json'
if((Get-FileHash -LiteralPath $manifest).Hash.ToLowerInvariant() -cne $ExpectedInputsSha256){throw 'Reviewed input manifest differs'}
$info=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WindowStyle='Hidden';$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
foreach($arg in @('-NoProfile','-File',(Join-Path $buildRoot 'Invoke-NativeProbe.ps1'),'-Mode','webview2-dom','-Execute','-ExpectedInputsSha256',$ExpectedInputsSha256,'-PauseBeforeResumeMilliseconds','5000')){$info.ArgumentList.Add($arg)}
$controller=[Diagnostics.Process]::Start($info)
$creation=$controller.StartTime.ToFileTimeUtc();$stdout=$controller.StandardOutput.ReadToEndAsync();$stderr=$controller.StandardError.ReadToEndAsync()
$receipt=$null;$clock=[Diagnostics.Stopwatch]::StartNew();$cut=$false
try {
    while($clock.ElapsedMilliseconds -lt 60000 -and -not $controller.HasExited){
        foreach($directory in Get-ChildItem -LiteralPath $buildRoot -Directory -Filter 'run-*'){
            $file=Join-Path $directory.FullName 'native-receipt.json'
            if(-not (Test-Path -LiteralPath $file)){continue}
            $value=Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
            if($value.controller_pid -eq $controller.Id -and $value.controller_creation_filetime -eq $creation -and $value.status -ceq 'running' -and @($value.events|Where-Object type -ceq 'owned_process').Count -gt 0){$receipt=$file;break}
        }
        if($receipt){
            if($controller.StartTime.ToFileTimeUtc() -ne $creation){throw 'Controller identity changed'}
            $controller.Kill();if(-not $controller.WaitForExit(10000)){throw 'Owned controller did not exit'};$cut=$true;break
        }
        Start-Sleep -Milliseconds 50
    }
    if(-not $cut){throw 'Exact owned live controller cut point unavailable'}
    $null=$stdout.GetAwaiter().GetResult();$errorText=$stderr.GetAwaiter().GetResult()
    if($errorText.Length -gt 0){throw 'Owned controller stderr before cut'}
    & (Join-Path $buildRoot 'Invoke-NativeProbe.ps1') -Mode reconcile -Execute -Receipt $receipt | Out-Null
    $result=Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
    if($result.outcome -cne 'owner_loss_recovered' -or -not $result.processes_drained -or (Test-Path -LiteralPath $result.root)){throw 'Owner-loss profile recovery failed'}
    [ordered]@{schema='cs3-web-owner-loss/1';controller_pid=$controller.Id;controller_creation_filetime=$creation;inputs_sha256=$ExpectedInputsSha256;receipt=$receipt;receipt_sha256=(Get-FileHash -LiteralPath $receipt).Hash.ToLowerInvariant();outcome=$result.outcome;processes_drained=$result.processes_drained}|ConvertTo-Json
} finally {
    if(-not $controller.HasExited){$controller.Kill();$null=$controller.WaitForExit(10000)}
    $controller.Dispose()
}
