# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'production-package.ps1')
$temporary=Join-Path ([IO.Path]::GetTempPath()) ('vcp-production-package-test-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$cases=@()
foreach($scenario in @('loose-result','traversal','duplicate','extra-entry','changed-archive')) {
    $case=Join-Path $temporary $scenario
    New-Item -ItemType Directory -Path $case | Out-Null
    $archive=Join-Path $case 'vcp-0.2.0-beta.1-windows-x64-unsigned.zip'
    $entries=switch($scenario) {
        'traversal' { @('../outside.txt') }
        'duplicate' { @('vcp.exe','VCP.EXE') }
        'extra-entry' { @('vcp.exe','manifest.json','unexpected.txt') }
        default { @('vcp.exe','manifest.json') }
    }
    $zip=[IO.Compression.ZipFile]::Open($archive,[IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach($name in $entries) {
            $stream=$zip.CreateEntry($name).Open()
            try { $bytes=[Text.Encoding]::UTF8.GetBytes('synthetic invalid package; never executed');$stream.Write($bytes,0,$bytes.Length) } finally {$stream.Dispose()}
        }
    } finally {$zip.Dispose()}
    $receipt=@{schema='vcp-distribution-result/1';status=if($scenario -eq 'loose-result'){'candidate'}else{'release-candidate'};
        package=[IO.Path]::GetFileName($archive);archive_sha256=(Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant();
        manifest=@{files=@(@{path='vcp.exe'})}}
    $result=Join-Path $case 'result.json'
    $receipt | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $result -Encoding utf8NoBOM
    if($scenario -eq 'changed-archive'){[IO.File]::AppendAllText($archive,'changed')}
    $expected=switch($scenario){'loose-result'{'Strict production'};'traversal'{'Unsafe or duplicate'};'duplicate'{'Unsafe or duplicate'};'extra-entry'{'inventory mismatch'};'changed-archive'{'archive digest mismatch'}}
    $caught=$null
    try { Read-ProductionPackage -PackageResult $result -ExtractionRoot (Join-Path $case 'extracted') | Out-Null }
    catch {$caught=$_.Exception.Message}
    if(-not $caught -or -not $caught.Contains($expected)){throw "Unexpected $scenario outcome: $caught"}
    if((Test-Path -LiteralPath (Join-Path $case 'extracted')) -or (Test-Path -LiteralPath (Join-Path $case 'outside.txt'))){throw 'Rejected archive produced extracted files'}
    $cases+=$scenario
}
@{schema='vcp-production-package-boundary-tests/1';status='pass';cases=$cases;evidence=$temporary}|ConvertTo-Json -Depth 4
