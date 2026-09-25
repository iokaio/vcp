# SPDX-License-Identifier: Apache-2.0
# Abrupt-owner-loss qualification helper; no product API.
param([Parameter(Mandatory)][string]$Node, [Parameter(Mandatory)][string]$NodeSha256,
    [Parameter(Mandatory)][string]$State)
$ErrorActionPreference = 'Stop'
if ((Get-FileHash -LiteralPath $Node).Hash.ToLowerInvariant() -cne $NodeSha256) { throw 'Node identity changed' }
Add-Type -Path (Join-Path $PSScriptRoot 'AppContainerFixture.cs')
$fixture = [Vcp.Qualification.AppContainerFixture]::new()
try {
    Copy-Item -LiteralPath $Node -Destination (Join-Path $fixture.Root 'node.exe')
    if ((Get-FileHash -LiteralPath (Join-Path $fixture.Root 'node.exe')).Hash.ToLowerInvariant() -cne $NodeSha256) { throw 'Copied Node identity changed' }
    [IO.File]::WriteAllText((Join-Path $fixture.Root 'package.json'), '{"type":"commonjs"}')
    @{ name = $fixture.Name; root = $fixture.Root } | ConvertTo-Json -Compress | Set-Content -LiteralPath $State
    $code = 'require("node:fs").writeFileSync("child.json", JSON.stringify({pid:process.pid})); setInterval(() => {}, 1000);'
    $null = $fixture.RunBounded((Join-Path $fixture.Root 'node.exe'), @('--no-addons', '-e', $code),
        $true, [byte[]]::new(0), 1024, 268435456, 30000, [Threading.CancellationToken]::None)
} finally { $fixture.Dispose() }
