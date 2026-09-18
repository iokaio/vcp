# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([Parameter(Mandatory)][string]$Config)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$request = Get-Content -LiteralPath $Config -Raw | ConvertFrom-Json
if ($request.mode -notin @('control-before', 'inference', 'missing', 'corrupt', 'control-after') -or
    $request.nonce -cnotmatch '^[a-f0-9]{32}$' -or $request.port -lt 1 -or $request.port -gt 65535) { throw 'Invalid qualification request' }
Add-Type -Path (Join-Path $PSScriptRoot 'AppContainerFixture.cs')
$fixture = [Vcp.Qualification.AppContainerFixture]::new()
$outcome = [ordered]@{ profile = $fixture.Name; root = $fixture.Root; cleanup = 'pending'; result = $null }
function Save-Outcome { $outcome | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $request.outcome -Encoding utf8 }
try {
    Save-Outcome
    $program = Join-Path $fixture.Root 'qualify.exe'
    Copy-Item -LiteralPath $request.binary -Destination $program
    $control = $request.mode.StartsWith('control-')
    $arguments = @('--network-control', $request.address, [string]$request.port, $request.nonce)
    if (-not $control) {
        $model = Join-Path $fixture.Root 'model'
        [IO.Directory]::CreateDirectory($model) | Out-Null
        foreach ($entry in $request.files) {
            $destination = [IO.Path]::GetFullPath((Join-Path $model $entry.path))
            if (-not $destination.StartsWith($model + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Asset escaped owned fixture' }
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
            Copy-Item -LiteralPath (Join-Path $request.assets $entry.path) -Destination $destination
            if ((Get-Item -LiteralPath $destination).Length -ne $entry.bytes -or
                (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw 'Copied public asset identity mismatch' }
        }
        $fault = Join-Path $model 'config.json'
        if ($request.mode -eq 'missing') { Remove-Item -LiteralPath $fault }
        if ($request.mode -eq 'corrupt') {
            $bytes = [IO.File]::ReadAllBytes($fault); $bytes[0] = $bytes[0] -bxor 1
            [IO.File]::WriteAllBytes($fault, $bytes)
        }
        $arguments = @('--offline', $model, $request.address, [string]$request.port, $request.nonce)
    }
    $outcome.result = $fixture.Run($program, $arguments, (-not $control), 30000)
} finally {
    try { $fixture.Dispose(); $outcome.cleanup = 'completed' }
    finally { Save-Outcome }
}
exit $outcome.result.ExitCode
