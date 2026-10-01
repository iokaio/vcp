// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { execFileSync } = require('node:child_process');

test('candidate clears OIDC minting capability before stage admission and child processes', { skip: process.platform !== 'win32' }, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-oidc-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const script = path.join(root, 'test.ps1');
  fs.writeFileSync(script, String.raw`param([string]$Candidate,[string]$Node,[string]$Root)
$ErrorActionPreference='Stop'
$env:ACTIONS_ID_TOKEN_REQUEST_URL='https://invalid.example/fixture-only'
$env:ACTIONS_ID_TOKEN_REQUEST_TOKEN='synthetic-fixture-not-a-credential'
$env:VCP_CANDIDATE_TEST_SENTINEL='preserved'
$failure=$null
try{& $Candidate -ReviewedCommit ('a'*40) -OutputRoot (Join-Path $Root 'uncreated') -Stage production-build -StopAfter portable-contracts}catch{$failure=$_.Exception.Message}
if($failure -cne 'Stage is beyond the selected stopping point'){throw ('Unexpected candidate admission result: '+$failure)}
& $Node -e 'if(process.env.ACTIONS_ID_TOKEN_REQUEST_URL || process.env.ACTIONS_ID_TOKEN_REQUEST_TOKEN || process.env.VCP_CANDIDATE_TEST_SENTINEL!=="preserved")process.exit(1)'
if($LASTEXITCODE -ne 0){throw 'Candidate leaked OIDC capability to later child or cleared unrelated environment'}
if(Test-Path -LiteralPath (Join-Path $Root 'uncreated')){throw 'Admission fixture unexpectedly started a stage'}
`);
  execFileSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', script, '-Candidate',
    path.resolve(__dirname, '../../../scripts/release/candidate.ps1'), '-Node', process.execPath, '-Root', root],
  { windowsHide: true, timeout: 15000, stdio: 'pipe' });
});

test('signed candidate refuses untrusted setup before execution and binds all installed signatures', { skip: process.platform !== 'win32' }, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-signatures-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const script = path.join(root, 'test.ps1');
  fs.writeFileSync(script, String.raw`param([string]$Helper,[string]$Root)
$ErrorActionPreference='Stop'
. $Helper
$script:fixtureRoot=$Root
function Require([bool]$Value,[string]$Message){if(-not $Value){throw $Message}}
function Hash([string]$File){(Get-FileHash -LiteralPath $File).Hash.ToLowerInvariant()}
$policy=@{status='signed';publisher='CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US';identity_eku='1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669'}
$candidate='c'*64
$registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1'
$installer=Join-Path $Root 'setup.exe';$archive=Join-Path $Root 'native.zip'
[IO.File]::WriteAllText($installer,'fixture setup bytes, never executed')
[IO.File]::WriteAllText($archive,'fixture archive bytes, never extracted')
$archiveHash=Hash $archive
$rowByRole=@{}
foreach($role in @('setup','engine','launcher','uninstaller')){
    $file=if($role -eq 'setup'){$installer}else{Join-Path $Root ($role+'.exe')}
    if($role -ne 'setup'){[IO.File]::WriteAllText($file,('fixture '+$role+' bytes, never executed'))}
    $rowByRole[$role]=@{role=$role;output_sha256=(Hash $file);output_bytes=(Get-Item -LiteralPath $file).Length}
}
$native=@{status='release-candidate';package='native.zip';archive_sha256=$archiveHash;manifest=@{
    release=@{candidate_id=$candidate;signing=$policy};files=@(@{path='vcp.exe';sha256=$rowByRole.engine.output_sha256})
    signing=@{status='signed';transformation=@{candidate_id=$candidate;files=@($rowByRole.engine,$rowByRole.launcher)}}}}
$setup=@{schema='vcp-setup-result/1';candidate_id=$candidate;native_archive_sha256=$archiveHash;launcher_sha256=$rowByRole.launcher.output_sha256
    archive=@{file='setup.exe';sha256=$rowByRole.setup.output_sha256}
    signing=@{status='signed';transformation=@{candidate_id=$candidate;files=@($rowByRole.setup,$rowByRole.uninstaller)}}}
$nativeResult=Join-Path $Root 'native.json';$setupResult=Join-Path $Root 'setup.json'
$native | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $nativeResult
$setup | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $setupResult
# Only registry observations, OS certificate results and the process boundary
# are synthetic. The real production helper hashes every actual fixture file.
function Test-Path([string]$LiteralPath){
    if($LiteralPath -ceq $registration){return $script:registered}
    return Microsoft.PowerShell.Management\Test-Path -LiteralPath $LiteralPath
}
function Get-ItemProperty([string]$LiteralPath){
    Require ($LiteralPath -ceq $registration) 'Unexpected registry access'
    return @{InstallLocation=$script:app}
}
function Get-AuthenticodeSignature([string]$LiteralPath){
    $role=if($LiteralPath -ceq $installer){'setup'}elseif($LiteralPath -eq $script:engine){'engine'}elseif([IO.Path]::GetFileName($LiteralPath) -eq 'unins000.exe'){'uninstaller'}else{'launcher'}
    $script:observed+=,$role
    $bad=$script:mode -eq ($role+'-invalid')
    $subject=if($script:mode -eq 'uninstaller-publisher' -and $role -eq 'uninstaller'){'CN=Other publisher'}else{$policy.publisher}
    $identity=if($script:mode -eq 'engine-eku' -and $role -eq 'engine'){'1.2.3'}else{$policy.identity_eku}
    $code=if($script:mode -eq 'engine-code-eku' -and $role -eq 'engine'){'1.2.4'}else{'1.3.6.1.5.5.7.3.3'}
    return @{Status=$(if($bad){'NotTrusted'}else{'Valid'});TimeStamperCertificate=$(if($script:mode -eq 'uninstaller-timestamp' -and $role -eq 'uninstaller'){$null}else{@{Subject='fixture timestamp'}})
        SignerCertificate=@{Subject=$subject;Extensions=@(@{Oid=@{Value='2.5.29.37'};EnhancedKeyUsages=@(@{Value=$identity},@{Value=$code})})}}
}
function Invoke-BetaProcess([string]$Executable,[string[]]$Arguments,[string]$Directory){
    $script:invocations++
    if($Executable -ceq $installer){
        Require (($script:observed -join ',') -ceq 'setup') 'Setup ran before signature admission'
        $script:app=Join-Path $Directory 'Program Files café'
        $script:engine=Join-Path $script:app "engine/releases/$archiveHash/vcp.exe"
        New-Item -ItemType Directory -Path (Split-Path -Parent $script:engine) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $script:fixtureRoot 'engine.exe') -Destination $script:engine
        Copy-Item -LiteralPath (Join-Path $script:fixtureRoot 'launcher.exe') -Destination (Join-Path $script:app 'vcp.exe')
        Copy-Item -LiteralPath (Join-Path $script:fixtureRoot 'uninstaller.exe') -Destination (Join-Path $script:app 'unins000.exe')
        if($script:mode -eq 'uninstaller-bytes'){[IO.File]::AppendAllText((Join-Path $script:app 'unins000.exe'),'changed')}
        if($script:mode -eq 'uninstaller-same-size'){
            $file=Join-Path $script:app 'unins000.exe';$bytes=[IO.File]::ReadAllBytes($file);$bytes[0]=$bytes[0] -bxor 1;[IO.File]::WriteAllBytes($file,$bytes)
        }
        $script:registered=$true
        return @{exit_code=0;stdout='';stderr=''}
    }
    Require ($Executable -ceq (Join-Path $script:app 'vcp.exe') -and ($Arguments -join '|') -ceq '--resolve-installation') 'Unexpected child invocation'
    Require (($script:observed -join ',') -ceq 'setup,launcher,uninstaller') 'Launcher executed before installed signature checks'
    return @{exit_code=0;stderr='';stdout=(@{schema='vcp-installed-engine/1';executable=$script:engine;data_directory=$script:data}|ConvertTo-Json -Compress)}
}
foreach($script:mode in @('valid','setup-invalid','launcher-invalid','engine-invalid','uninstaller-invalid','uninstaller-publisher','uninstaller-timestamp','engine-eku','engine-code-eku','uninstaller-bytes','uninstaller-same-size')){
    $script:registered=$false;$script:observed=@();$script:invocations=0;$script:engine=$null
    $script:data=Join-Path $Root ($script:mode+' private-data');New-Item -ItemType Directory -Path $script:data | Out-Null
    $sentinel=Join-Path $script:data 'retained.txt';[IO.File]::WriteAllText($sentinel,'retained data')
    $result=$null;$failure=$null
    try{$result=Install-BetaCandidate $nativeResult $setupResult (Join-Path $Root $script:mode) $script:data}catch{$failure=$_.Exception.Message}
    if($script:mode -eq 'valid'){
        Require (-not $failure -and $result.signatures.Count -eq 4) ('Valid signed installation refused: '+$failure)
        Require (($result.signatures.role -join ',') -ceq 'setup,launcher,uninstaller,engine') 'Incomplete signed evidence'
    }elseif($script:mode -eq 'setup-invalid'){
        Require ($failure -like 'Candidate setup lacks*' -and $script:invocations -eq 0 -and -not $script:registered) 'Untrusted setup was executed'
    }else{
        Require ($null -eq $result -and $failure -like 'Candidate *') ('Changed signed component admitted: '+$failure)
        Require $script:registered 'Failed installed observation lost repair state'
        if($script:mode -like 'uninstaller-*' -or $script:mode -eq 'launcher-invalid'){Require ($script:invocations -eq 1) 'Rejected component reached launcher execution'}
    }
    Require ([IO.File]::ReadAllText($sentinel) -ceq 'retained data') 'Retained data changed'
}
`);
  execFileSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', script, '-Helper',
    path.resolve(__dirname, '../../../scripts/release/candidate-runtime.ps1'), '-Root', root],
  { windowsHide: true, timeout: 20000, stdio: 'pipe' });
});
