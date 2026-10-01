# SPDX-License-Identifier: Apache-2.0
# A real, bounded compiler probe of owned MSVC helper cleanup. No Cargo build.
#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
if (-not $IsWindows) { throw 'Native Windows MSVC supervision smoke required' }
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $PSScriptRoot 'build-progress.ps1')

function Get-SmokeHash([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}
function Assert-SmokePath([string]$Path) {
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '[\x00-\x1f]') { throw 'Ordinary absolute local path required' }
    $full=[IO.Path]::GetFullPath($Path)
    foreach ($part in $Path.Replace('/','\').Substring(3).Split('\')) {
        if (-not $part -or $part -in @('.','..') -or $part -match '[<>:"|?*]|[. ]$' -or
            $part -match '^(?i:con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)') { throw 'Unsafe smoke path component' }
    }
    for ($cursor=$full;$cursor;$cursor=[IO.Path]::GetDirectoryName($cursor)) {
        $item=Get-Item -LiteralPath $cursor -Force -ErrorAction SilentlyContinue
        if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Redirected smoke path refused' }
        if ($item -and $cursor -ine $full -and -not $item.PSIsContainer) { throw 'Non-directory smoke path ancestor' }
    }
    return $full
}
function Save-SmokeReceipt($Value,[string]$Path) {
    $Value | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}

$output=Assert-SmokePath (Join-Path $repo ('artifacts/msvc-supervision-smoke/'+[guid]::NewGuid()))
if (Test-Path -LiteralPath $output) { throw 'Fresh unique smoke output required' }
New-Item -ItemType Directory -Path $output | Out-Null
$receiptPath=Join-Path $output 'receipt.json'
$receipt=[ordered]@{schema='vcp-msvc-supervision-smoke/1';status='fail';started_at=[DateTime]::UtcNow.ToString('o');
    scope='Synthetic MSVC compile/link and owned helper supervision; no product build or installation';output=$output;
    script_sha256=(Get-SmokeHash $PSCommandPath);supervisor_sha256=(Get-SmokeHash (Join-Path $PSScriptRoot 'build-progress.ps1'));
    timeout_seconds=90;failures=@();supervision=$null;outputs=@()}
Save-SmokeReceipt $receipt $receiptPath
try {
    foreach ($name in @('CL','_CL_','LINK','_LINK_')) {
        if ([Environment]::GetEnvironmentVariable($name,'Process')) { throw 'Ambient MSVC option overrides refused for the synthetic probe' }
    }
    $vswhere=Assert-SmokePath (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe')
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw 'Visual Studio discovery tool is missing' }
    $installation=@(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    if ($LASTEXITCODE -ne 0 -or $installation.Count -ne 1 -or -not $installation[0]) { throw 'One installed native Visual C++ x64 toolchain required' }
    $visualStudio=Assert-SmokePath $installation[0]
    $launch=Assert-SmokePath (Join-Path $visualStudio 'Common7/Tools/Launch-VsDevShell.ps1')
    & $launch -Arch amd64 -HostArch amd64 -SkipAutomaticLocation *> (Join-Path $output 'environment.log')
    $compiler=Assert-SmokePath (Get-Command cl -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    if (-not $compiler.StartsWith($visualStudio.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Compiler does not belong to selected Visual Studio installation' }
    $telemetry=Assert-SmokePath (Join-Path (Split-Path -Parent $compiler) 'vctip.exe')
    if (-not (Test-Path -LiteralPath $telemetry -PathType Leaf)) { throw 'Exact compiler-sibling MSVC telemetry helper required' }
    $receipt.tools=@{vswhere=@{path=$vswhere;sha256=(Get-SmokeHash $vswhere)};
        developer_shell=@{path=$launch;sha256=(Get-SmokeHash $launch)};
        compiler=@{path=$compiler;sha256=(Get-SmokeHash $compiler)};
        telemetry=@{path=$telemetry;sha256=(Get-SmokeHash $telemetry)}}
    $source=Join-Path $output 'main.cpp'
    [IO.File]::WriteAllText($source,"int main() { return 0; }`n",[Text.UTF8Encoding]::new($false))
    $arguments=@('/nologo','/Zi','/FS','/Od','/Fomain.obj','/Fdcompiler.pdb','/Femain.exe','main.cpp',
        '/link','/DEBUG','/PDB:main.pdb','/INCREMENTAL:NO')
    $command=[ordered]@{executable=$compiler;arguments=$arguments;directory=$output;
        msvc_telemetry_executable=$telemetry;timeout_seconds=90;source=@{path=$source;sha256=(Get-SmokeHash $source)}}
    $commandPath=Join-Path $output 'command.json';Save-SmokeReceipt $command $commandPath
    $receipt.command_sha256=Get-SmokeHash $commandPath
    $receipt.source=$command.source
    Save-SmokeReceipt $receipt $receiptPath
    $receipt.supervision=Invoke-VcpBuildProcess -Executable $compiler -Arguments $arguments -WorkingDirectory $output `
        -LogPath (Join-Path $output 'compiler.log') -ProgressPath (Join-Path $output 'progress.json') `
        -TimeoutSeconds 90 -ProgressSeconds 5 -MsvcTelemetryExecutable $telemetry
    Save-SmokeReceipt $receipt $receiptPath
    if ($receipt.supervision.exit_code -ne 0 -or $receipt.supervision.process_exit_code -ne 0 -or
        $receipt.supervision.broker_exit_code -ne 0 -or $receipt.supervision.timed_out -ne $false -or
        $receipt.supervision.forced_cleanup -ne $false -or $receipt.supervision.job_active_processes_zero -ne $true -or
        $receipt.supervision.child_exit_observation_removed -ne $true) { throw 'Compiler or owned process supervision did not succeed' }
    $cleanup=@($receipt.supervision.planned_service_cleanup)
    if ($receipt.supervision.completion -ceq 'planned-service-cleanup') {
        if ($cleanup.Count -ne 1 -or $cleanup[0].result -cne 'terminated' -or $cleanup[0].termination_exit_code -ne 1 -or
            $cleanup[0].process_id -le 0 -or $cleanup[0].sha256 -cne $receipt.tools.telemetry.sha256 -or
            (Assert-SmokePath $cleanup[0].image) -ine $telemetry) { throw 'Planned cleanup must identify only the selected owned telemetry helper' }
    } elseif ($receipt.supervision.completion -cne 'natural' -or $cleanup.Count -ne 0) {
        throw 'Compiler completion must distinguish natural exit from explicit owned telemetry cleanup'
    }
    foreach ($name in @('main.obj','compiler.pdb','main.exe','main.pdb')) {
        $file=Assert-SmokePath (Join-Path $output $name)
        $item=Get-Item -LiteralPath $file -ErrorAction Stop
        if ($item.PSIsContainer -or $item.Length -le 0) { throw 'Required compile/link output missing or empty' }
        $receipt.outputs+=@{name=$name;bytes=$item.Length;sha256=(Get-SmokeHash $file)}
    }
    foreach ($tool in $receipt.tools.Values) {
        if ((Get-SmokeHash $tool.path) -cne $tool.sha256) { throw 'Selected compiler/discovery/helper bytes changed during observation' }
    }
    if ((Get-SmokeHash $source) -cne $receipt.source.sha256 -or
        (Get-SmokeHash $commandPath) -cne $receipt.command_sha256 -or
        (Get-SmokeHash $PSCommandPath) -cne $receipt.script_sha256 -or
        (Get-SmokeHash (Join-Path $PSScriptRoot 'build-progress.ps1')) -cne $receipt.supervisor_sha256) { throw 'Smoke source, command or supervisor changed during observation' }
    $receipt.status='pass'
} catch { $receipt.failures+=@{reason=$_.Exception.Message} }
finally {
    $receipt.logs=@(Get-ChildItem -LiteralPath $output -File -Filter '*.log' | ForEach-Object {@{name=$_.Name;bytes=$_.Length;sha256=(Get-SmokeHash $_.FullName)}})
    $receipt.completed_at=[DateTime]::UtcNow.ToString('o')
    Save-SmokeReceipt $receipt $receiptPath
}
Write-Output $receiptPath
if ($receipt.status -cne 'pass') { throw 'MSVC supervision smoke failed; retained receipt and logs describe the failure' }
