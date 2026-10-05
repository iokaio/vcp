#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-source-checkpoint-' + [guid]::NewGuid().ToString('N'))
$workspace = Join-Path $root 'workspace'; $run = Join-Path $root 'run'
[void][IO.Directory]::CreateDirectory($workspace); [void][IO.Directory]::CreateDirectory($run)
$checks = 0
function Check($condition, $message) { if (-not $condition) { throw $message }; $script:checks++ }
function Reject([scriptblock]$action, $message) { $failed = $false; try { & $action } catch { $failed = $true }; Check $failed $message }
$ctx = @{ Name = 'checkpoint'; Workspace = $workspace; Root = $run; ReuseProject = $true; ProgressLog = Join-Path $run 'progress.log'; Git = (Get-Command git).Source }
$junction = Join-Path $workspace 'linked'
try {
    [IO.File]::WriteAllText((Join-Path $workspace 'source.txt'), 'staged source')
    & git -C $workspace init -q
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git initialization failed' }
    & git -C $workspace add -- source.txt
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git staging failed' }
    $index = Get-Sha256 (Join-Path $workspace '.git/index'); $head = Get-Sha256 (Join-Path $workspace '.git/HEAD')
    [IO.File]::WriteAllText((Join-Path $workspace 'source.txt'), "unstaged café`r`nsecond line`n")
    [IO.File]::WriteAllBytes((Join-Path $workspace 'untracked.bin'), [byte[]]@(0, 255, 1, 13, 10))
    foreach ($directory in 'node_modules', 'dist', 'bin', 'obj', 'models', 'models-repro', 'reports') {
        [void][IO.Directory]::CreateDirectory((Join-Path $workspace $directory))
        [IO.File]::WriteAllText((Join-Path $workspace "$directory/excluded.txt"), 'generated')
    }
    [void][IO.Directory]::CreateDirectory((Join-Path $workspace 'src/Inventory.Web/Models'))
    [IO.File]::WriteAllText((Join-Path $workspace 'src/Inventory.Web/Models/Product.cs'), 'public record Product(int Id);')
    [void][IO.Directory]::CreateDirectory((Join-Path $workspace 'src/Inventory.Web/Pages/Reports'))
    [IO.File]::WriteAllText((Join-Path $workspace 'src/Inventory.Web/Pages/Reports/Index.cshtml'), '@page')
    $sourceHash = Get-Sha256 (Join-Path $workspace 'source.txt')
    Save-Checkpoint $ctx 'passed reused stage'
    $manifests = @(Get-ChildItem (Join-Path $run 'checkpoints') -Recurse -Filter manifest.json)
    Check ($manifests.Count -eq 1) 'Reused stage did not publish one source snapshot'
    $manifestPath = $manifests[0].FullName; $manifestHash = Get-Sha256 $manifestPath
    $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json -AsHashtable
    Check ($manifest.schema -eq 'vcp-source-checkpoint/1' -and $manifest.message -eq 'passed reused stage' -and $manifest.workspace -eq $workspace -and $manifest.at) 'Snapshot provenance missing'
    Check ($manifest.file_count -eq 4 -and $manifest.files.Count -eq 4) 'Dependencies, builds, or Git metadata entered the authored snapshot'
    Check ($manifest.files.Contains('src/Inventory.Web/Models/Product.cs') -and $manifest.exclusions.root_only_directory_names -contains 'models') 'Authored nested Models source or exclusion provenance was lost'
    Check ($manifest.files.Contains('src/Inventory.Web/Pages/Reports/Index.cshtml') -and $manifest.exclusions.root_only_directory_names -contains 'reports') 'Authored Pages/Reports source or exclusion provenance was lost'
    Check (-not (Get-WorkspaceManifest $workspace).Contains('src/Inventory.Web/Models/Product.cs')) 'Checkpoint mode changed existing workspace-diff exclusion behavior'
    foreach ($relative in $manifest.files.Keys) {
        $copy = Join-Path $manifests[0].DirectoryName "files/$relative"
        Check ((Get-Sha256 $copy) -eq $manifest.files[$relative] -and (Get-Sha256 (Join-Path $workspace $relative)) -eq $manifest.files[$relative]) 'Restorable bytes differ from their source/manifest hash'
    }
    Check ((Get-Sha256 (Join-Path $workspace '.git/index')) -eq $index -and (Get-Sha256 (Join-Path $workspace '.git/HEAD')) -eq $head) 'Reused snapshot modified Git index or HEAD'
    Check ((Get-Sha256 (Join-Path $workspace 'source.txt')) -eq $sourceHash) 'Snapshot edited application source'
    Save-Checkpoint $ctx 'second passed stage'
    Check (@(Get-ChildItem (Join-Path $run 'checkpoints') -Recurse -Filter manifest.json).Count -eq 2) 'Checkpoints were not unique'
    Check ((Get-Sha256 $manifestPath) -eq $manifestHash) 'Second checkpoint overwrote the first'
    $unsafe = $ctx.Clone(); $unsafe.Root = Join-Path $workspace 'nested-run'
    Reject { Save-Checkpoint $unsafe 'unsafe destination' } 'Checkpoint nested in source was accepted'

    # Inject source mutation at the second manifest read, after copying; no
    # completion manifest may be published for that failed checkpoint.
    & $module {
        param($workspace)
        $script:checkpointOriginalManifest = (Get-Command Get-WorkspaceManifest).ScriptBlock
        $script:checkpointWorkspace = $workspace; $script:checkpointReads = 0
        function script:Get-WorkspaceManifest {
            param($Path, [switch]$ForCheckpoint)
            if ($Path -eq $script:checkpointWorkspace) {
                $script:checkpointReads++
                if ($script:checkpointReads -eq 2) { [IO.File]::AppendAllText((Join-Path $Path 'source.txt'), 'concurrent mutation') }
            }
            & $script:checkpointOriginalManifest $Path -ForCheckpoint:$ForCheckpoint
        }
    } $workspace
    Reject { Save-Checkpoint $ctx 'mutating source' } 'Concurrent source mutation was accepted'
    Check (@(Get-ChildItem (Join-Path $run 'checkpoints') -Recurse -Filter manifest.json).Count -eq 2) 'Failed capture published a completion manifest'
    & $module { Set-Item function:script:Get-WorkspaceManifest $script:checkpointOriginalManifest }
    [void][IO.Directory]::CreateDirectory((Join-Path $root 'link-target'))
    New-Item -ItemType Junction -Path $junction -Target (Join-Path $root 'link-target') | Out-Null
    Reject { Save-Checkpoint $ctx 'linked source' } 'Source junction was accepted'
    Remove-Item -LiteralPath $junction -Force
    & $module {
        function script:Get-WorkspaceManifest { param($Path, [switch]$ForCheckpoint); return [ordered]@{ '../escape.txt' = ('a' * 64) } }
    }
    Reject { Save-Checkpoint $ctx 'escaping manifest' } 'Manifest traversal was accepted'
    Check (@(Get-ChildItem (Join-Path $run 'checkpoints') -Recurse -Filter manifest.json).Count -eq 2) 'Rejected link/path escape published a snapshot'
    Write-Host "Source checkpoint checks passed: $checks"
}
finally {
    if (Test-Path -LiteralPath $junction) { Remove-Item -LiteralPath $junction -Force }
    Remove-Module $module -Force
    $full = [IO.Path]::GetFullPath($root)
    $prefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-source-checkpoint-*') { throw 'Unsafe checkpoint test cleanup path' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
