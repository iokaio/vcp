# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([Parameter(Mandatory)][string]$Config)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$request = Get-Content -LiteralPath $Config -Raw | ConvertFrom-Json
if ($request.mode -notin @('control-before', 'restricted', 'control-after') -or
    $request.nonce -cnotmatch '^[a-f0-9]{32}$' -or $request.port -lt 1 -or $request.port -gt 65535) { throw 'Invalid execution fixture request' }
Add-Type -Path (Join-Path $PSScriptRoot 'AppContainerFixture.cs')
$integrityBefore = [Vcp.Qualification.AppContainerFixture]::CurrentIntegrity()
$fixture = [Vcp.Qualification.AppContainerFixture]::new()
$outcome = [ordered]@{ cleanup = 'pending'; result = $null; integrity_before = $integrityBefore; integrity_after = [Vcp.Qualification.AppContainerFixture]::CurrentIntegrity() }
$inside = [IO.Path]::GetFullPath((Join-Path $fixture.Root 'work'))
$junction = [IO.Path]::GetFullPath((Join-Path $inside 'redirect'))
if (-not $junction.StartsWith($fixture.Root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid fixture junction' }
try {
    $outside = [IO.Path]::GetFullPath($request.outside)
    $expectedOutside = [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $Config) $request.mode))
    if ($outside -cne $expectedOutside -or (Get-Item -LiteralPath $outside).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected outside canary directory' }
    [IO.Directory]::CreateDirectory($inside) | Out-Null
    # An executable in AC can produce a low-integrity control process too. Both
    # canaries deliberately permit low-integrity writes, so the comparison tests
    # AppContainer identity/ACL enforcement rather than mandatory-label denial.
    & icacls $inside /grant ('*' + $fixture.Sid + ':(OI)(CI)M') | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot grant the owned inside canary' }
    $ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    foreach ($canary in @($inside, $outside)) {
        & icacls $canary /grant ('*' + $ownerSid + ':(OI)(CI)F') | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Cannot grant the owner of the disposable canary' }
        & icacls $canary /setintegritylevel '(OI)(CI)L' | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Cannot label the disposable canary' }
    }
    $program = Join-Path $fixture.Root 'qualify.exe'
    Copy-Item -LiteralPath $request.binary -Destination $program
    & icacls $program /grant ('*' + $ownerSid + ':F') | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot grant the owned fixture executable' }
    & icacls $program /setintegritylevel M | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot label the owned fixture executable' }
    # Simulate replacing a previously prepared in-root directory with a junction.
    [IO.Directory]::CreateDirectory($junction) | Out-Null
    [IO.Directory]::Delete($junction)
    New-Item -Path $junction -ItemType Junction -Target $request.outside | Out-Null
    $arguments = @('boundary', $inside, $outside, ('127.0.0.1:' + $request.port), $request.nonce)
    $outcome.result = $fixture.Run($program, $arguments, ($request.mode -eq 'restricted'), 15000)
} finally {
    try {
        if (Test-Path -LiteralPath $junction) {
            if (-not ((Get-Item -LiteralPath $junction).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Fixture junction was unexpectedly replaced' }
            Remove-Item -LiteralPath $junction -Force
        }
        $fixture.Dispose()
        $outcome.cleanup = 'completed'
    } finally {
        $outcome | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $request.outcome -Encoding utf8
    }
}
exit $outcome.result.ExitCode
