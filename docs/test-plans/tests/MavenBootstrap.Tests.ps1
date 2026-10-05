#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise C's real resolver with synthetic downloads and archives, without inference.
$ErrorActionPreference = 'Stop'
$scenario = Join-Path $PSScriptRoot '../scenario-c-java-ledger-cli.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($scenario, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Scenario C does not parse.' }
$definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Resolve-LedgerMavenHome' }, $true)
if (-not $definition) { throw 'Maven resolver is missing.' }
. ([scriptblock]::Create($definition.Extent.Text))
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-maven-bootstrap-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function Get-Command { param($Name, $CommandType, $ErrorAction) if ($script:installed) { [pscustomobject]@{ Source = $script:installed } } }
function Write-Step { param($Ctx, $Message, $Level) }
function Invoke-WebRequest {
    param($Uri, $OutFile, $TimeoutSec)
    $script:downloads++
    Check ($Uri -ceq 'https://archive.apache.org/dist/maven/maven-3/3.9.16/binaries/apache-maven-3.9.16-bin.zip') 'Download source or pinned version changed without updating qualification.'
    Check ($TimeoutSec -eq 120) 'Toolchain download must have a bounded timeout.'
    if ($script:mode -eq 'offline') { throw 'fixture network unavailable' }
    Copy-Item -LiteralPath $script:fixtureArchive -Destination $OutFile
}
function Get-FileHash {
    param($LiteralPath, $Algorithm)
    Check ($Algorithm -eq 'SHA512') 'Archive must use SHA512.'
    Check (Test-Path -LiteralPath $LiteralPath -PathType Leaf) 'Hash check must follow the download.'
    if ($script:mode -eq 'bad-hash') { return Microsoft.PowerShell.Utility\Get-FileHash -LiteralPath $LiteralPath -Algorithm $Algorithm }
    # Synthetic accepted archive; the real pinned bytes are checked by the live dry run.
    [pscustomobject]@{ Hash = 'ED41650D42485CFC243FAD22158CAF9CBB5DC408CE7A09DDB94DD42A019DE929CA43065BFA450612CF12BF78B5CAFA3884B96C090DE326FF590448C933454AF3' }
}
function New-Context([string]$Name) {
    $root = Join-Path $testRoot $Name
    $temporary = Join-Path $root 'tmp'
    New-Item -ItemType Directory -Path $temporary -Force | Out-Null
    @{ Root = $root; Temp = $temporary; Notes = [Collections.Generic.List[string]]::new() }
}
function Check-Rejected($Ctx, [string]$Pattern) {
    $message = $null
    try { Resolve-LedgerMavenHome $Ctx | Out-Null } catch { $message = $_.Exception.Message }
    Check ($null -ne $message -and $message -like $Pattern) "Expected rejection $Pattern; got $message"
    Check ($Ctx.Notes.Count -eq 0) 'Failed setup claimed a successful provision.'
}
try {
    $fixture = Join-Path $testRoot 'fixture/apache-maven-3.9.16/bin'
    New-Item -ItemType Directory -Path $fixture -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $fixture 'mvn.cmd') -Value 'fixture launcher'
    $fixtureArchive = Join-Path $testRoot 'fixture.zip'
    Compress-Archive -LiteralPath (Split-Path -Parent $fixture) -DestinationPath $fixtureArchive
    $originalPath = $env:Path
    $downloads = 0; $mode = 'accepted'
    $installed = Join-Path $testRoot 'existing Maven/bin/mvn.cmd'
    $ctx = New-Context 'installed'
    Check ((Resolve-LedgerMavenHome $ctx) -eq (Join-Path $testRoot 'existing Maven')) 'Existing PATH installation was not preserved.'
    Check ($downloads -eq 0 -and $ctx.Notes.Count -eq 0) 'Installed Maven triggered provisioning.'

    $installed = $null
    $ctx = New-Context 'missing'
    $resolved = Resolve-LedgerMavenHome $ctx
    Check ($resolved -eq (Join-Path $ctx.Root 'toolchains/apache-maven-3.9.16')) 'Bootstrap escaped the run tool directory.'
    Check (Test-Path -LiteralPath (Join-Path $resolved 'bin/mvn.cmd')) 'Verified archive was not extracted.'
    Check ($ctx.Notes.Count -eq 1 -and $ctx.Notes[0] -match 'verified SHA512') 'Bootstrap provenance was not recorded.'
    Check ($env:Path -ceq $originalPath) 'Bootstrap modified the user process environment.'

    $mode = 'bad-hash'; $ctx = New-Context 'bad-hash'
    Check-Rejected $ctx '*SHA512*Install Maven*'
    Check (-not (Test-Path -LiteralPath (Join-Path $ctx.Root 'toolchains'))) 'Unverified archive was extracted.'
    $mode = 'offline'; $ctx = New-Context 'offline'
    Check-Rejected $ctx '*network unavailable*Install Maven*'
    Check (-not (Test-Path -LiteralPath (Join-Path $ctx.Root 'toolchains'))) 'Failed download changed toolchain files.'

    $mode = 'accepted'; $ctx = New-Context 'invalid-layout'
    $fixtureArchive = Join-Path $testRoot 'invalid.zip'
    Set-Content -LiteralPath (Join-Path $testRoot 'unrelated.txt') -Value 'not a Maven distribution'
    Compress-Archive -LiteralPath (Join-Path $testRoot 'unrelated.txt') -DestinationPath $fixtureArchive
    Check-Rejected $ctx '*expected binary distribution*Install Maven*'
    Check ($env:Path -ceq $originalPath) 'Failed bootstrap modified PATH.'
    Write-Host "Maven bootstrap passed: $checks assertions."
}
finally {
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing cleanup outside temporary root.' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
