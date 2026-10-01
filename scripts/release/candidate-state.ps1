# SPDX-License-Identifier: Apache-2.0
# State for sequential stages in one owned candidate job. This is not an
# artifact-import or cross-run resume mechanism; stage commands remain in code.
#requires -Version 7.0

function Get-CandidateStageIds {
    return @('source-gate','provision','portable-contracts','production-build',
        'native-package','setup-package','vsix-package','pair','native-boundaries',
        'installed-native','installed-editor')
}

function Assert-CandidateStatePath([string]$Path,[bool]$Directory) {
    if ([string]::IsNullOrWhiteSpace($Path) -or $Path -cnotmatch '^[A-Za-z]:\\' -or
        -not [IO.Path]::IsPathFullyQualified($Path)) { throw 'Candidate state requires an absolute ordinary drive path' }
    $full=[IO.Path]::GetFullPath($Path)
    if ($full.TrimEnd('\') -ine $Path.TrimEnd('\')) { throw 'Candidate state refuses noncanonical paths' }
    foreach ($part in $full.Substring(3).Split('\',[StringSplitOptions]::RemoveEmptyEntries)) {
        if ($part -match '[<>:"|?*\x00-\x1f]' -or $part -match '[. ]$' -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'Candidate state refuses unsafe path components' }
    }
    $item=Get-Item -LiteralPath $full -Force -ErrorAction Stop
    if ([bool]$item.PSIsContainer -ne $Directory) { throw 'Candidate state path has the wrong file type' }
    for ($current=$item; $current; $current=if ($current -is [IO.DirectoryInfo]) {$current.Parent} else {$current.Directory}) {
        if ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Candidate state refuses redirected paths or ancestors' }
    }
    return $item.FullName
}

function Save-CandidateRun($Run,[string]$RunPath) {
    if ($Run -isnot [Collections.IDictionary]) { throw 'Candidate state must be a dictionary' }
    $root=Assert-CandidateStatePath $Run.output_root $true
    $expected=Join-Path $root 'run.json'
    if ($RunPath -ine $expected) { throw 'Candidate state may only write its owned run.json' }
    $existing=Get-Item -LiteralPath $expected -Force -ErrorAction SilentlyContinue
    if ($existing) { $null=Assert-CandidateStatePath $expected $false }
    # Serialize before opening any file. A serialization/write failure preserves
    # the previous checkpoint. Rename within the same directory is atomic.
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($Run | ConvertTo-Json -Depth 100)+[Environment]::NewLine)
    $temporary=Join-Path $root ('.run-'+[Guid]::NewGuid().ToString('N')+'.tmp')
    try {
        $stream=[IO.FileStream]::new($temporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
        $null=Assert-CandidateStatePath $root $true
        $null=Assert-CandidateStatePath $temporary $false
        $existing=Get-Item -LiteralPath $expected -Force -ErrorAction SilentlyContinue
        if ($existing) { $null=Assert-CandidateStatePath $expected $false }
        [IO.File]::Move($temporary,$expected,$true)
    } finally {
        if ([IO.File]::Exists($temporary)) {
            $null=Assert-CandidateStatePath $temporary $false
            [IO.File]::Delete($temporary)
        }
    }
}

function Assert-CandidateStateDigest([string]$Path,$Digest) {
    if ($Digest -isnot [string] -or $Digest -cnotmatch '^[a-f0-9]{64}$' -or
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() -cne $Digest) {
        throw 'Candidate state file digest mismatch'
    }
}

function Assert-CandidateContinuation($Run,[string]$OutputRoot,[string]$Repository,
    [string]$ReviewedCommit,[string]$StageId,[string]$StopAfter) {
    $ids=@(Get-CandidateStageIds)
    $next=[array]::IndexOf($ids,$StageId)
    $stop=[array]::IndexOf($ids,$StopAfter)
    if ($StageId -cnotin $ids -or $next -le 0 -or
        $StopAfter -cnotin @('portable-contracts','production-build','pair','installed-editor') -or $next -gt $stop) {
        throw 'Candidate continuation requires a noninitial stage within the selected scope'
    }
    if ($Run -isnot [Collections.IDictionary] -or $Run.schema -cne 'vcp-candidate-run/1' -or
        $Run.status -cne 'running' -or $ReviewedCommit -cnotmatch '^[a-f0-9]{40}$' -or
        $Run.reviewed_commit -cne $ReviewedCommit -or $Run.stop_after -cne $StopAfter -or
        $Run.receipts -isnot [Collections.IDictionary] -or $Run.receipt_sha256 -isnot [Collections.IDictionary]) {
        throw 'Candidate continuation identity or status mismatch'
    }
    $root=Assert-CandidateStatePath $OutputRoot $true
    $repo=Assert-CandidateStatePath $Repository $true
    if ($Run.output_root -isnot [string] -or $Run.repository_root -isnot [string] -or
        $Run.output_root -ine $root -or $Run.repository_root -ine $repo) { throw 'Candidate continuation roots mismatch' }
    $null=Assert-CandidateStatePath (Join-Path $root 'run.json') $false
    if ($Run.stages -isnot [array] -or $Run.stages.Count -ne $next) { throw 'Candidate continuation requires the exact prior stage prefix' }
    for ($index=0; $index -lt $next; $index++) {
        $row=$Run.stages[$index]
        if ($row -isnot [Collections.IDictionary] -or $row.id -cne $ids[$index] -or $row.status -cne 'pass' -or
            ($row.exit_code -isnot [int] -and $row.exit_code -isnot [long]) -or $row.exit_code -ne 0) {
            throw 'Candidate continuation requires successful ordered prior stages'
        }
        $expected=Join-Path $root ('logs/'+$ids[$index]+'.log')
        if ($row.log -isnot [string] -or $row.log -ine $expected) { throw 'Candidate prior log is outside its fixed stage location' }
        $log=Assert-CandidateStatePath $row.log $false
        Assert-CandidateStateDigest $log $row.log_sha256
    }
    $produced=@{build='production-build';native='native-package';setup='setup-package';vsix='vsix-package'}
    foreach ($name in $produced.Keys) {
        $required=[array]::IndexOf($ids,$produced[$name]) -lt $next
        if ($required -ne $Run.receipts.Contains($name)) { throw 'Candidate receipts do not match completed production stages' }
    }
    if ($Run.receipts.Count -ne $Run.receipt_sha256.Count) { throw 'Candidate receipt hashes do not match receipt entries' }
    foreach ($name in $Run.receipts.Keys) {
        if ($name -cnotin @('build','native','setup','vsix','delivery') -or -not $Run.receipt_sha256.Contains($name)) {
            throw 'Candidate state contains an unknown or unbound receipt'
        }
        if ($Run.receipts[$name] -isnot [string]) { throw 'Candidate receipt path must be a string' }
        $receipt=Assert-CandidateStatePath $Run.receipts[$name] $false
        switch -CaseSensitive ($name) {
            'build' { $parent='build'; $leaf='build-receipt.json'; $guid='^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$' }
            'native' { $parent='native'; $leaf='result.json'; $guid='^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$' }
            'setup' { $parent='setup'; $leaf='setup-result.json'; $guid='^[a-f0-9]{32}$' }
            'vsix' { $expected=Join-Path $root 'vsix/manifest.json' }
            'delivery' { $expected=Join-Path $repo 'artifacts/beta-gate/delivery.json' }
        }
        if ($name -cin @('build','native','setup')) {
            $id=Split-Path -Leaf (Split-Path -Parent $receipt)
            if ($id -cnotmatch $guid) { throw 'Candidate receipt has an invalid owned directory identity' }
            $expected=Join-Path $root "$parent/$id/$leaf"
        }
        if ($receipt -ine $expected) { throw 'Candidate receipt is outside its fixed owned location' }
        Assert-CandidateStateDigest $receipt $Run.receipt_sha256[$name]
    }
    $pairIndex=[array]::IndexOf($ids,'pair')
    if ($pairIndex -lt $next) {
        $pair=Assert-CandidateStatePath (Join-Path $root 'pair.json') $false
        Assert-CandidateStateDigest $pair $Run.stages[$pairIndex].verified_pair_sha256
    }
}
