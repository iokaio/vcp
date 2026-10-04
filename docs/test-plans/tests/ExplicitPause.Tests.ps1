#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Offline control-boundary proof; no native VCP or inference is launched.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-explicit-pause-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$checks = 0
function Check($Value, $Message) { if (-not $Value) { throw $Message }; $script:checks++ }
function New-Pause { @{ attempted = $false; acknowledged = $false; scope = $null; trigger_event = $null; receipt = $null; error = $null } }
function Observe($Frame, $Pause) { & $module { param($c, $f, $p) Invoke-ScenarioPauseOnProgress $c 'T5' $f $p } @{ Logs = $root } $Frame $Pause }
$scope = @{ workspace = 'workspace'; session = 'session'; task = 'task' }
function Progress {
    @{ type = 'event'; scope = $scope; event = @{ event = @{ id = 'event'; kind = 'effect_transition'; data = @{ facts = @(@{ collection = 'effect'; value = @{ state = 'succeeded' } }) } } } }
}
try {
    & $module {
        $script:pauseCalls = 0; $script:pauseMode = 'success'
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, [string[]]$Arguments, $TimeoutSeconds, [switch]$DenyProviderCredentials)
            if (($Arguments -join ' ') -ne 'tasks pause task' -or -not $DenyProviderCredentials -or $TimeoutSeconds -ne 30) {
                throw 'Pause changed scope, credential isolation or control-request containment.'
            }
            $script:pauseCalls++
            if ($script:pauseMode -eq 'throw') { throw 'Control channel unavailable.' }
            $receipt = @{ version = 1; workspace = 'workspace'; command = 'command'; transaction = 'transaction'; digest = ('a' * 64); result = @{ result = 'accepted' } }
            $result = @{ ExitCode = 0; TimedOut = $false; InvalidLines = 0; Result = @{ data = $receipt } }
            switch ($script:pauseMode) {
                'foreign' { $receipt.workspace = 'foreign' }
                'malformed' { $result.InvalidLines = 1 }
                'timeout' { $result.TimedOut = $true }
                'rejected' { $receipt.result.result = 'rejected' }
                'digest' { $receipt.digest = 'bad' }
                'missing' { $result.Result = $null }
            }
            return $result
        }
    }
    $pause = New-Pause
    Observe (Progress) $pause
    Check (-not $pause.attempted) 'Progress without accepted scope triggered control.'
    Observe @{ type = 'accepted'; scope = $scope } $pause
    foreach ($case in 'child', 'foreign-workspace', 'foreign-session', 'pending', 'failed', 'unknown', 'other-kind') {
        $frame = Progress | ConvertTo-Json -Depth 20 | ConvertFrom-Json -AsHashtable
        switch ($case) {
            'child' { $frame.scope.task = 'child' }
            'foreign-workspace' { $frame.scope.workspace = 'foreign' }
            'foreign-session' { $frame.scope.session = 'foreign' }
            'other-kind' { $frame.event.event.kind = 'task_transition' }
            default { $frame.event.event.data.facts[0].value.state = $case }
        }
        Observe $frame $pause
        Check (-not $pause.attempted) "$case triggered the root progress pause."
    }
    Observe (Progress) $pause
    Observe (Progress) $pause
    Check ($pause.acknowledged -and $pause.trigger_event -eq 'event') 'Successful pause lost its exact progress trigger and receipt.'
    Check ((& $module { $script:pauseCalls }) -eq 1) 'Repeated progress sent another stop command.'
    foreach ($mode in 'throw', 'foreign', 'malformed', 'timeout', 'rejected', 'digest', 'missing') {
        & $module { param($m) $script:pauseMode = $m } $mode
        $pause = New-Pause
        Observe @{ type = 'accepted'; scope = $scope } $pause
        $before = & $module { $script:pauseCalls }
        Observe (Progress) $pause
        Observe (Progress) $pause
        Check ($pause.attempted -and -not $pause.acknowledged -and $pause.error) "$mode falsely acknowledged a durable pause."
        Check ((& $module { $script:pauseCalls }) -eq ($before + 1)) "$mode retried a control operation without outcome proof."
        $evidence = Get-Content (Join-Path $root 'T5/explicit-pause.json') -Raw | ConvertFrom-Json
        Check (-not $evidence.acknowledged -and $evidence.error) "$mode failed to retain diagnostic evidence."
    }
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-explicit-pause-*') { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Host "Explicit pause checks passed: $checks"
