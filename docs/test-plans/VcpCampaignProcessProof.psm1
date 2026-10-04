# SPDX-License-Identifier: Apache-2.0
# Financial quarantine only. A failed process never becomes a passing task.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -DisableNameChecking

function Assert-ProcessProofPath([string]$Path) {
    $probe = [IO.Path]::GetFullPath($Path)
    while ($probe) {
        if ((Test-Path -LiteralPath $probe) -and ((Get-Item -LiteralPath $probe -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Process proof cannot traverse a link.' }
        $parent = Split-Path -Parent $probe
        if ($parent -eq $probe) { break }; $probe = $parent
    }
}

function Read-ProcessProofArtifact($Descriptor, [string]$ScenarioRoot) {
    if ($Descriptor.state -ne 'complete' -or [string]$Descriptor.length -notmatch '^\d+$' -or
        [decimal]$Descriptor.length -gt 16777216 -or $Descriptor.sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $Descriptor.spec.id -notmatch '^[0-9a-f-]{36}$' -or @($Descriptor.retained).Count -ne 1 -or
        $Descriptor.retained[0].start -ne '0' -or $Descriptor.retained[0].end -ne $Descriptor.length -or
        @($Descriptor.spec.omissions | Where-Object { $_ -notin 'authentication_headers', 'recovery_material' }).Count) { throw 'Process artifact is incomplete or unsupported.' }
    $workspaces = Join-Path $ScenarioRoot 'vcp-data/workspaces'
    Assert-ProcessProofPath $workspaces
    $matches = @(Get-ChildItem -LiteralPath $workspaces -Directory | ForEach-Object {
        $path = Join-Path $_.FullName "canonical/spool/$($Descriptor.spec.id)"
        if (Test-Path -LiteralPath $path -PathType Container) { $path }
    })
    if ($matches.Count -ne 1) { throw 'Process artifact storage identity is ambiguous or missing.' }
    Assert-ProcessProofPath $matches[0]
    $chunks = @(Get-ChildItem -LiteralPath $matches[0] -File -Filter '*.chunk' | Sort-Object Name)
    if ($chunks.Count -gt 10000) { throw 'Process artifact chunk limit exceeded.' }
    $files = @(); $stream = [IO.MemoryStream]::new()
    try {
        for ($index = 0; $index -lt $chunks.Count; $index++) {
            $file = $chunks[$index]; Assert-ProcessProofPath $file.FullName
            if ($file.Name -cnotmatch '^(\d{20})-([0-9a-f]{64})\.chunk$' -or [decimal]$Matches[1] -ne $index -or
                $file.Length -gt 16777216 -or $stream.Length + $file.Length -gt 16777216) { throw 'Process artifact chunk sequence or size invalid.' }
            $expected = $Matches[2]; $bytes = [IO.File]::ReadAllBytes($file.FullName)
            if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant() -cne $expected) { throw 'Process artifact chunk hash mismatch.' }
            $stream.Write($bytes, 0, $bytes.Length); $files += @{ path = $file.FullName; sha256 = $expected }
        }
        $bytes = $stream.ToArray()
    }
    finally { $stream.Dispose() }
    if ($bytes.Length -ne [long]$Descriptor.length -or [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant() -cne $Descriptor.sha256) { throw 'Process artifact aggregate hash mismatch.' }
    return @{ bytes = $bytes; files = $files }
}

function Assert-SourceProofMap($Sources, $Manifest, [string]$WorkspaceId) {
    $seen = @{}
    if (@($Sources).Count -ne $Manifest.Count) { throw 'Observed authored source inventory is incomplete.' }
    foreach ($source in $Sources) {
        if (-not $source.path -or $seen.ContainsKey($source.path) -or -not $Manifest.Contains($source.path) -or
            $source.root -ne $WorkspaceId -or $source.sha256 -cne $Manifest[$source.path]) { throw 'Observed authored source differs from checkpoint.' }
        $seen[$source.path] = $true
    }
}

function Assert-CampaignFailedProcess($Effect, $Bundle, [string]$ScenarioRoot, [string]$Workspace) {
    if (-not $ScenarioRoot -or -not $Workspace -or $Effect.state -ne 'failed' -or
        [string]$Effect.exit_code -notmatch '^-?\d+$' -or [long]$Effect.exit_code -eq 0 -or -not $Effect.execution -or
        @($Effect.observed_changes).Count -ne 5 -or @($Effect.observed_changes | Sort-Object -Unique).Count -ne 5) { throw 'Failed effect lacks an exact terminal process proof.' }
    $scope = $Bundle.task.scope
    foreach ($field in 'task','session','workspace') { if (-not $scope[$field] -or $Effect.scope[$field] -ne $scope[$field]) { throw 'Failed process scope mismatch.' } }
    $items = @($Bundle.views.tools | ForEach-Object items)
    $artifacts = @{}; $files = @()
    foreach ($id in $Effect.observed_changes) {
        $matches = @($items | Where-Object { $_.collection -eq 'artifact' -and $_.id -eq $id -and $_.visibility -eq 'available' })
        if ($matches.Count -ne 1 -or $matches[0].record.spec.id -ne $id) { throw 'Missing or ambiguous process artifact.' }
        $descriptor = $matches[0].record
        foreach ($field in 'task','session','workspace') { if ($descriptor.spec.scope[$field] -ne $scope[$field]) { throw 'Process artifact scope mismatch.' } }
        $key = if ($descriptor.spec.channel -in 'stdout','stderr') { $descriptor.spec.channel } else { $descriptor.spec.schema }
        if ($key -notin 'stdout','stderr','vcp-prepared-tool-v2','vcp-process-start-v1','vcp-process-outcome-v1' -or $artifacts.ContainsKey($key) -or
            ($key -notin 'stdout','stderr' -and $descriptor.spec.channel -ne 'evidence')) { throw 'Unexpected or duplicate process artifact kind.' }
        $read = Read-ProcessProofArtifact $descriptor $ScenarioRoot; $files += $read.files
        $artifacts[$key] = @{ descriptor = $descriptor; bytes = $read.bytes }
    }
    foreach ($key in 'vcp-prepared-tool-v2','vcp-process-start-v1','vcp-process-outcome-v1') {
        $artifacts[$key].value = [Text.UTF8Encoding]::new($false,$true).GetString($artifacts[$key].bytes) | ConvertFrom-Json -AsHashtable -Depth 100
    }
    $prepared = $artifacts['vcp-prepared-tool-v2'].value.prepared
    $start = $artifacts['vcp-process-start-v1'].value
    $outcome = $artifacts['vcp-process-outcome-v1'].value
    $document = [Text.Json.JsonDocument]::Parse([Text.UTF8Encoding]::new($false,$true).GetString($artifacts['vcp-prepared-tool-v2'].bytes))
    try { $operationBytes = [Text.Encoding]::UTF8.GetBytes($document.RootElement.GetProperty('prepared').GetProperty('operation').GetRawText()) }
    finally { $document.Dispose() }
    if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($operationBytes)).ToLowerInvariant() -cne $Effect.operation_digest -or
        $prepared.operation.tool -ne 'vcp_exec' -or $prepared.operation.invocation.kind -ne 'process' -or $prepared.operation.invocation.shell -ne $false) { throw 'Prepared operation does not match the failed native process.' }
    foreach ($field in 'task','session','workspace') { if ($prepared.operation.scope[$field] -ne $scope[$field]) { throw 'Prepared process scope mismatch.' } }
    if ($start.execution -ne $Effect.execution -or $start.identity_authority -ne 'owned process/job handles; PID is diagnostic only' -or
        [string]$start.process_id -notmatch '^\d+$' -or $start.process_id -le 0 -or $start.job_processes -lt 1 -or
        $start.process_identity.pid -ne $start.process_id -or $start.process_identity.status -ne 'observed' -or
        $outcome.schema_version -ne 1 -or $outcome.effect -ne $Effect.id -or $outcome.execution -ne $Effect.execution -or
        $outcome.exit_code -ne $Effect.exit_code -or $outcome.owned_processes_remaining -ne 0 -or
        $outcome.output_complete -isnot [bool] -or $outcome.output_complete -ne $true -or $outcome.stop_reason -or
        $outcome.observed_workspace.complete -isnot [bool] -or $outcome.observed_workspace.complete -ne $true) { throw 'Process exit, identity, output or job quiescence is not proved.' }
    foreach ($channel in 'stdout','stderr') {
        if ([string]$outcome["${channel}_bytes"] -notmatch '^\d+$' -or [long]$outcome["${channel}_bytes"] -ne $artifacts[$channel].bytes.Length) { throw 'Process output bytes are incomplete.' }
    }
    $started = [DateTime]::FromFileTimeUtc([long]$start.process_identity.created)
    $checkpointRoot = Join-Path $ScenarioRoot 'checkpoints'; Assert-ProcessProofPath $checkpointRoot
    $candidates = @(Get-ChildItem -LiteralPath $checkpointRoot -Directory | ForEach-Object {
        Assert-ProcessProofPath $_.FullName
        $path = Join-Path $_.FullName 'manifest.json'
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            Assert-ProcessProofPath $path
            $manifest = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable -Depth 30
            if ($manifest.schema -eq 'vcp-source-checkpoint/1' -and $manifest.workspace -ieq $Workspace -and
                [DateTimeOffset]$manifest.at -le $started -and $manifest.file_count -gt 0 -and $manifest.file_count -eq $manifest.files.Count) {
                @{ path = $path; manifest = $manifest; at = [DateTimeOffset]$manifest.at }
            }
        }
    } | Sort-Object at -Descending)
    if (-not $candidates.Count) { throw 'No pre-process authored checkpoint proves isolation.' }
    $checkpoint = $candidates[0]; $manifest = $checkpoint.manifest.files
    $snapshot = Join-Path (Split-Path $checkpoint.path -Parent) 'files'
    foreach ($directory in $snapshot, $Workspace) {
        Assert-ProcessProofPath $directory
        $observed = Get-WorkspaceManifest -Path $directory -ForCheckpoint
        if ((Compare-WorkspaceManifest $manifest $observed).Changed -ne 0 -or @($observed.Values | Where-Object { $_ -cnotmatch '^[0-9a-f]{64}$' }).Count) { throw 'Authored checkpoint or current workspace changed.' }
    }
    Assert-SourceProofMap $prepared.sources.sources $manifest $scope.workspace
    Assert-SourceProofMap $outcome.observed_workspace.sources $manifest $scope.workspace
    $files += @{ path = $checkpoint.path; sha256 = Get-Sha256 $checkpoint.path }
    foreach ($relative in $manifest.Keys) {
        foreach ($directory in $snapshot, $Workspace) { $files += @{ path = Join-Path $directory $relative; sha256 = $manifest[$relative] } }
    }
    return @{ schema = 'vcp-financial-quarantine-terminal-process/1'; effect = $Effect.id; execution = $Effect.execution; scope = $scope
        exit_code = $Effect.exit_code; application_success = $false; owned_processes_remaining = 0; checkpoint = $checkpoint.path
        external_effects = $outcome.external_effects; limitation = 'External effects remain opaque; original workspace must remain permanently isolated. This proof changes financial eligibility only.'
        evidence_files = $files }
}

function Assert-CampaignProcessProofFiles($Proof, [string]$ScenarioRoot, [string]$Workspace) {
    if ($Proof.schema -ne 'vcp-financial-quarantine-terminal-process/1' -or $Proof.application_success -ne $false -or
        $Proof.owned_processes_remaining -ne 0 -or -not @($Proof.evidence_files).Count) { throw 'Original terminal process proof is invalid.' }
    foreach ($file in $Proof.evidence_files) {
        $path = [IO.Path]::GetFullPath($file.path)
        $inside = $false
        foreach ($root in $ScenarioRoot, $Workspace) {
            if ($path.StartsWith([IO.Path]::GetFullPath($root).TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { $inside = $true }
        }
        if (-not $inside) { throw 'Terminal process proof source escapes its isolated evidence/workspace.' }
        Assert-ProcessProofPath $path
        if ((Get-Sha256 $path) -cne $file.sha256) { throw 'Original terminal process evidence changed.' }
    }
}

Export-ModuleMember -Function Assert-CampaignFailedProcess, Assert-CampaignProcessProofFiles
