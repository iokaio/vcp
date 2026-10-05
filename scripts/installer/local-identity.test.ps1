# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
# Load only the pure identity boundary; never execute installation/PATH operations.
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'shell.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw $errors[0] }
$function=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -ceq 'Assert-CandidateIdentity'},$true)
. ([scriptblock]::Create($function.Extent.Text))
$id='a'*64
$release=@{release=@{candidate_id=$id}}
$local=@{artifact='unsigned-local-candidate';signing=@{status='unsigned'};local_candidate=@{
    schema='vcp-local-candidate/1';status='unsigned-local-candidate';signing='unsigned';qualification='not-release-qualified'
    candidate_id=$id;native_version='0.2.6';sdk_version='0.2.6';vsix_version='0.2.6'
}}
Assert-CandidateIdentity $release $id Release
Assert-CandidateIdentity $local $id UnsignedLocal
$cases=@(
    { Assert-CandidateIdentity $local $id Release },
    { Assert-CandidateIdentity $release $id UnsignedLocal },
    { Assert-CandidateIdentity $local ('b'*64) UnsignedLocal },
    { $local.local_candidate.vsix_version='0.2.5'; Assert-CandidateIdentity $local $id UnsignedLocal }
)
foreach ($case in $cases) {
    $rejected=$false
    try { & $case } catch { $rejected=$true }
    if (-not $rejected) { throw 'Invalid identity unexpectedly accepted' }
}
Write-Output 'PASS: local/release identities accepted only in their explicit mode; mismatches rejected (6 checks).'
$builder=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../build-local-setup.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw $errors[0] }
$versionFunction=$builder.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -ceq 'Assert-SetupProductVersion'},$true)
. ([scriptblock]::Create($versionFunction.Extent.Text))
foreach ($actual in @('0.2.6','0.2.6   ','0.2.6.0   ')) { Assert-SetupProductVersion $actual '0.2.6' }
foreach ($actual in @('0.2.5   ','0.2.60','0.2.6-local','')) {
    $rejected=$false
    try { Assert-SetupProductVersion $actual '0.2.6' } catch { $rejected=$true }
    if (-not $rejected) { throw 'Incorrect installer version unexpectedly accepted' }
}
Write-Output 'PASS: padded Inno versions accepted; different or malformed product versions rejected (7 checks).'
