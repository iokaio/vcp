#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1') -Force
Import-Module (Join-Path $PSScriptRoot '../VcpCampaignProcessProof.psm1') -Force -DisableNameChecking
$checks = 0
function Check($Value, $Message) { if (-not $Value) { throw $Message }; $script:checks++ }
function Reject([scriptblock]$Action, $Message) { $failed=$false; try { & $Action | Out-Null } catch { $failed=$true }; Check $failed $Message }
function Sha([byte[]]$Bytes) { [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant() }
function JsonBytes($Value) { ,([Text.Encoding]::UTF8.GetBytes(($Value | ConvertTo-Json -Depth 50 -Compress))) }
function Artifact($F, [string]$Schema, [string]$Channel, [byte[]]$Bytes) {
    $id=[guid]::NewGuid().ToString(); $hash=Sha $Bytes
    $dir=Join-Path $F.root "vcp-data/workspaces/fixture/canonical/spool/$id"
    [void][IO.Directory]::CreateDirectory($dir)
    $path=Join-Path $dir ('00000000000000000000-'+$hash+'.chunk'); [IO.File]::WriteAllBytes($path,$Bytes)
    $descriptor=@{state='complete';length=[string]$Bytes.Length;sha256=$hash;retained=@(@{start='0';end=[string]$Bytes.Length})
        spec=@{id=$id;schema=$Schema;channel=$Channel;scope=$F.scope;omissions=@('authentication_headers','recovery_material')} }
    return @{collection='artifact';id=$id;visibility='available';record=$descriptor;fixture_path=$path}
}
function Fixture([switch]$ShortPaths) {
    $base=Join-Path $env:TEMP ('vcp-terminal-quarantine-'+[guid]::NewGuid().ToString('N'))
    $directoryBase=$base
    if ($ShortPaths) {
        [void][IO.Directory]::CreateDirectory($base)
        # Hosted Windows TEMP can contain an 8.3 alias. On volumes without
        # short-name generation, this still exercises the same proof boundary.
        $fso=New-Object -ComObject Scripting.FileSystemObject
        try { $directoryBase=$fso.GetFolder($base).ShortPath }
        finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($fso) }
        Write-Host "Short-path alias differs: $($directoryBase -ine $base)"
    }
    $root=Join-Path $directoryBase 'scenario'; $workspace=Join-Path $directoryBase 'workspace'
    [void][IO.Directory]::CreateDirectory($workspace)
    [IO.File]::WriteAllText((Join-Path $workspace 'README.md'),'original source')
    $hash=(Get-FileHash -LiteralPath (Join-Path $workspace 'README.md')).Hash.ToLowerInvariant()
    $scope=@{task='task';session='session';workspace='workspace'}
    $effect=@{id='effect';scope=$scope;state='failed';execution='execution';exit_code=1;observed_changes=@();operation_digest=''}
    $sources=@(@{path='README.md';sha256=$hash;root='workspace'})
    $operation=@{scope=$scope;tool='vcp_exec';invocation=@{kind='process';shell=$false}}
    $f=@{base=$base;root=$root;workspace=$workspace;scope=$scope;effect=$effect;sources=$sources
        prepared=@{prepared=@{operation=$operation;sources=@{sources=$sources}}}
        start=@{execution='execution';identity_authority='owned process/job handles; PID is diagnostic only';job_processes=1;process_id=42;process_identity=@{pid=42;status='observed';created=[DateTime]::UtcNow.ToFileTimeUtc()}}
        outcome=@{schema_version=1;effect='effect';execution='execution';exit_code=1;owned_processes_remaining=0;output_complete=$true;stop_reason=$null;stdout_bytes=4;stderr_bytes=0;external_effects='opaque';observed_workspace=@{complete=$true;sources=$sources}}
    }
    $cp=Join-Path $root 'checkpoints/captured'; [void][IO.Directory]::CreateDirectory((Join-Path $cp 'files'))
    Copy-Item -LiteralPath (Join-Path $workspace 'README.md') -Destination (Join-Path $cp 'files/README.md')
    $f.checkpoint=Join-Path $cp 'manifest.json'
    @{schema='vcp-source-checkpoint/1';workspace=$workspace;at=[DateTimeOffset]::UtcNow.AddMinutes(-1).ToString('o');file_count=1;files=@{'README.md'=$hash}} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $f.checkpoint
    return $f
}
function Seal($F) {
    $F.effect.operation_digest=Sha (JsonBytes $F.prepared.prepared.operation)
    $artifacts=@((Artifact $F 'vcp-prepared-tool-v2' 'evidence' (JsonBytes $F.prepared)),
        (Artifact $F 'vcp-process-start-v1' 'evidence' (JsonBytes $F.start)),
        (Artifact $F 'vcp-process-outcome-v1' 'evidence' (JsonBytes $F.outcome)),
        (Artifact $F 'retained-full-output/1' 'stdout' ([Text.Encoding]::UTF8.GetBytes('fail'))),
        (Artifact $F 'retained-full-output/1' 'stderr' ([byte[]]@())))
    $F.artifacts=$artifacts; $F.effect.observed_changes=@($artifacts.id)
    $F.bundle=@{task=@{scope=$F.scope};views=@{tools=@(@{items=@($artifacts)})}}
}
$allRoots=[Collections.Generic.List[string]]::new()
try {
    $f=Fixture; $allRoots.Add($f.base); Seal $f
    $proof=Assert-CampaignFailedProcess $f.effect $f.bundle $f.root $f.workspace
    Check ($proof.application_success -eq $false -and $proof.exit_code -eq 1 -and $proof.external_effects -eq 'opaque') 'Failed/opaque outcome was promoted to success.'
    Assert-CampaignProcessProofFiles $proof $f.root $f.workspace; $checks++
    $f=Fixture -ShortPaths; $allRoots.Add($f.base); Seal $f
    $proof=Assert-CampaignFailedProcess $f.effect $f.bundle $f.root $f.workspace
    Check ($proof.exit_code -eq 1 -and -not $proof.application_success) 'Short-path failed outcome was promoted to success.'
    Assert-CampaignProcessProofFiles $proof $f.root $f.workspace; $checks++
    [IO.File]::WriteAllText((Join-Path $f.workspace 'README.md'),'changed through short path')
    Reject {Assert-CampaignFailedProcess $f.effect $f.bundle $f.root $f.workspace} 'Short-path source mutation was accepted.'
    foreach($mutation in @(
        {param($f) $f.effect.state='unknown'},
        {param($f) $f.effect.state='cancelled'},
        {param($f) $f.effect.exit_code=$null},
        {param($f) $f.effect.exit_code=0},
        {param($f) $f.outcome.output_complete=$false},
        {param($f) $f.outcome.owned_processes_remaining=1},
        {param($f) $f.outcome.stop_reason='timeout'},
        {param($f) $f.outcome.execution='different'},
        {param($f) $f.outcome.effect='different'},
        {param($f) $f.outcome.exit_code=2},
        {param($f) $f.outcome.stdout_bytes=3},
        {param($f) $f.outcome.observed_workspace.complete=$false},
        {param($f) $f.outcome.observed_workspace.sources=@()},
        {param($f) $f.prepared.prepared.sources.sources=@()},
        {param($f) $f.prepared.prepared.operation.invocation.shell=$true},
        {param($f) $f.start.execution='different'},
        {param($f) $f.start.process_identity.pid=99},
        {param($f) $f.start.identity_authority='pid alone'},
        {param($f) [IO.File]::WriteAllText((Join-Path $f.workspace 'README.md'),'changed')},
        {param($f) [IO.File]::WriteAllText((Join-Path $f.workspace 'added.txt'),'unexpected')},
        {param($f) [IO.File]::WriteAllText((Join-Path (Split-Path $f.checkpoint) 'files/README.md'),'changed')},
        {param($f) $m=Get-Content -Raw $f.checkpoint|ConvertFrom-Json; $m.at=[DateTimeOffset]::UtcNow.AddHours(1).ToString('o'); $m|ConvertTo-Json -Depth 8|Set-Content $f.checkpoint}
    )) {
        $f=Fixture; $allRoots.Add($f.base); & $mutation $f; Seal $f
        Reject {Assert-CampaignFailedProcess $f.effect $f.bundle $f.root $f.workspace} 'Incomplete/unknown/mutated failed process was accepted.'
    }
    foreach($mutation in @(
        {param($f) [IO.File]::WriteAllText($f.artifacts[2].fixture_path,'tampered')},
        {param($f) $f.artifacts[2].record.spec.scope=@{task='other';session='session';workspace='workspace'}},
        {param($f) $f.artifacts[2].record.state='aborted'},
        {param($f) $f.artifacts[2].record.retained[0].end='1'},
        {param($f) $f.effect.operation_digest='a'*64},
        {param($f) $f.effect.observed_changes=$f.effect.observed_changes[0..3]},
        {param($f) $f.effect.observed_changes[4]=$f.effect.observed_changes[3]},
        {param($f) $f.artifacts[1].visibility='hidden'},
        {param($f) $f.artifacts[0].record.spec.channel='stdout'}
    )) {
        $f=Fixture; $allRoots.Add($f.base); Seal $f; & $mutation $f
        Reject {Assert-CampaignFailedProcess $f.effect $f.bundle $f.root $f.workspace} 'Tampered/unlinked artifact was accepted.'
    }
    $f=Fixture; $allRoots.Add($f.base); Seal $f
    Reject {Assert-CampaignFailedProcess $f.effect $f.bundle '' $f.workspace} 'Missing physical proof was accepted.'
    $proof=Assert-CampaignFailedProcess $f.effect $f.bundle $f.root $f.workspace
    [IO.File]::WriteAllText((Join-Path $f.workspace 'README.md'),'later mutation')
    Reject {Assert-CampaignProcessProofFiles $proof $f.root $f.workspace} 'Original quarantine source mutation was ignored.'
    Write-Host "Terminal process quarantine checks passed: $checks"
}
finally {
    foreach($path in $allRoots) {
        $resolved=[IO.Path]::GetFullPath($path)
        if (-not $resolved.StartsWith([IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or
            (Split-Path $resolved -Leaf) -notlike 'vcp-terminal-quarantine-*') { throw 'Unsafe fixture cleanup path.' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
