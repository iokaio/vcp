# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# CS-3 prospective DOC remediation: source-frozen, genuine no-run qualification build.
# Historical cs3-comparison-build.ps1 remains unchanged; this schema never claims its binary identity.
[CmdletBinding()]
param(
    [switch]$CaptureOnly,
    [string]$SourceManifestPath,
    [string]$SourceManifestSha256
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows qualification build required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$target = Join-Path $repository 'artifacts/codex-target'
$workspace = Join-Path $repository 'src/third_party/codex/codex-rs'
if ($CaptureOnly) {
    if ($SourceManifestPath -or $SourceManifestSha256) { throw 'Capture-only mode does not accept a prior source manifest' }
} elseif (-not [IO.Path]::IsPathFullyQualified($SourceManifestPath) -or $SourceManifestSha256 -cnotmatch '^[a-f0-9]{64}$') {
    throw 'An absolute frozen source manifest and its SHA-256 are required before building'
}
$node = (Get-Command node -CommandType Application | Select-Object -First 1).Source
$node = & $node -p 'require("node:fs").realpathSync(process.execPath)'
if ($LASTEXITCODE -ne 0 -or -not [IO.Path]::IsPathFullyQualified($node)) { throw 'Physical Node executable unavailable' }
$cargoLauncher = (Get-Command cargo -CommandType Application | Select-Object -First 1).Source
$rustupLauncher = (Get-Command rustup -CommandType Application | Select-Object -First 1).Source
function Hash([string]$File) { (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant() }
function RegularPath([string]$File) {
    for ($part = [IO.Path]::GetFullPath($File); $part; $part = Split-Path -Parent $part) {
        if ((Test-Path -LiteralPath $part) -and ((Get-Item -Force -LiteralPath $part).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Build path has a reparse ancestor' }
    }
}
RegularPath $target
RegularPath $workspace
RegularPath $node
RegularPath $cargoLauncher
RegularPath $rustupLauncher
if (Test-Path -LiteralPath (Join-Path $workspace 'target')) { throw 'Generated target within frozen upstream is not allowed' }
# Cargo is deliberately invoked from the repository, matching the existing
# qualification build. --manifest-path does not change config discovery cwd.
$configCandidates = @()
for ($ancestor = $repository; $ancestor; $ancestor = Split-Path -Parent $ancestor) {
    $configCandidates += (Join-Path $ancestor '.cargo/config'), (Join-Path $ancestor '.cargo/config.toml')
}
$taskCargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$configCandidates += (Join-Path $taskCargoHome 'config'), (Join-Path $taskCargoHome 'config.toml')
# A new inherited config needs review rather than silently changing the recipe.
if (@($configCandidates | Where-Object { Test-Path -LiteralPath $_ }).Count) { throw 'Unreviewed inherited Cargo configuration' }
$overrides = @('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_TARGET','CARGO_BUILD_RUSTFLAGS','CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS','CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER','CC','CXX','CL','_CL_','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','AR','ARFLAGS','CMAKE_GENERATOR','CMAKE_TOOLCHAIN_FILE','CMAKE_ARGS')
if (@(Get-ChildItem Env: | Where-Object { $_.Value -and ($_.Name -in $overrides -or $_.Name -match '^CARGO_PROFILE_' -or $_.Name -match '^(CC|CXX|CFLAGS|CXXFLAGS|CPPFLAGS|LDFLAGS|AR|ARFLAGS)_') }).Count) { throw 'Unreviewed build environment override' }
$scope = @('src/crates','src/evals','src/tests/fixtures','src/skills/builtin',
    'src/third_party/codex/codex-rs/Cargo.toml','src/third_party/codex/codex-rs/Cargo.lock',
    'src/third_party/codex/codex-rs/rust-toolchain.toml','src/third_party/codex/codex-rs/.cargo',
    'src/third_party/munarium/server/src','src/third_party/munarium/server/Cargo.toml','src/third_party/munarium/server/Cargo.lock',
    'src/third_party/components','src/third_party/upstreams.toml','scripts/upstream','src/tests/support',
    'scripts/skills/builtin-assets.cjs','scripts/evals','src/tests/package.json','src/tests/package-lock.json')
$capture = @'
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const root=process.cwd(),scope=process.argv.slice(1),files=[],directories=[];let count=0,total=0;
function visit(relative,depth=0){if(++count>20000||depth>24)throw Error('Build source entry bound');const file=path.join(root,relative),s=fs.lstatSync(file);if(s.isSymbolicLink())throw Error('Linked build source');if(s.isDirectory()){directories.push(relative);for(const n of fs.readdirSync(file).sort())visit(relative+'/'+n,depth+1);}else{if(!s.isFile()||s.size>32*1024*1024)throw Error('Build source file bound');const b=fs.readFileSync(file);if(b.length!==s.size||(total+=b.length)>256*1024*1024)throw Error('Build source changed or aggregate bound');files.push({path:relative,bytes:b.length,sha256:sha(b)});}}
scope.forEach(p=>visit(p));console.log(JSON.stringify({scope,files,directories,content_sha256:sha(JSON.stringify({files,directories}))}));
'@
function Capture-Source {
    $value = & $node -e $capture @scope
    if ($LASTEXITCODE -ne 0) { throw 'Build source identity capture failed' }
    return $value
}
function Capture-Toolchain {
    $identities = @()
    foreach ($name in @('cargo','rustc')) {
        $file = & $rustupLauncher which --toolchain 1.98.0 $name
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $file -PathType Leaf)) { throw 'Exact Rust toolchain unavailable' }
        RegularPath $file
        $version = & $file --version --verbose
        if ($LASTEXITCODE -ne 0) { throw 'Toolchain version unavailable' }
        $identities += [ordered]@{name=$name;path=$file;sha256=Hash $file;version=@($version)}
    }
    $identities += [ordered]@{name='node';path=$node;sha256=Hash $node;version=@(& $node --version)}
    $identities += [ordered]@{name='cargo-launcher';path=$cargoLauncher;sha256=Hash $cargoLauncher}
    $identities += [ordered]@{name='rustup-launcher';path=$rustupLauncher;sha256=Hash $rustupLauncher}
    # Record installed native compiler/linker candidates without claiming a
    # cached Cargo invocation necessarily reruns any of them.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    RegularPath $vswhere
    $identities += [ordered]@{name='vswhere';path=$vswhere;sha256=Hash $vswhere}
    $installations = @(& $vswhere -all -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    if ($LASTEXITCODE -ne 0 -or -not $installations.Count) { throw 'Native compiler installation unavailable' }
    foreach ($installation in $installations | Sort-Object) {
        foreach ($version in Get-ChildItem -LiteralPath (Join-Path $installation 'VC/Tools/MSVC') -Directory | Sort-Object Name) {
            foreach ($name in @('cl.exe','link.exe','lib.exe')) {
                $native = Join-Path $version.FullName ('bin/Hostx64/x64/' + $name)
                RegularPath $native
                $identities += [ordered]@{name=$name;path=$native;sha256=Hash $native}
            }
        }
    }
    return ConvertTo-Json -InputObject $identities -Depth 5 -Compress
}
if ($CaptureOnly) {
    $captureDirectory = Join-Path $repository ('artifacts/cs3-document-remediation-source/' + [guid]::NewGuid().ToString())
    RegularPath $captureDirectory
    Push-Location $repository
    try {
        $captured = Capture-Source
        New-Item -ItemType Directory -Path $captureDirectory | Out-Null
        $manifestFile = Join-Path $captureDirectory 'source-inputs.json'
        [IO.File]::WriteAllText($manifestFile,$captured,[Text.UTF8Encoding]::new($false))
        [ordered]@{schema='cs3-document-remediation-source/1';source_manifest=[ordered]@{path=$manifestFile;sha256=Hash $manifestFile};provider_calls=0;tests_executed=0;build_executed=$false} | ConvertTo-Json -Depth 4
    } finally { Pop-Location }
    return
}
RegularPath $SourceManifestPath
if (-not (Test-Path -LiteralPath $SourceManifestPath -PathType Leaf) -or (Get-Item -LiteralPath $SourceManifestPath).Length -gt 8MB -or (Hash $SourceManifestPath) -cne $SourceManifestSha256) { throw 'Frozen source manifest missing, oversized or changed' }
$sourceManifest = [IO.File]::ReadAllText($SourceManifestPath)
$directory = Join-Path $repository ('artifacts/cs3-document-remediation-build/' + [guid]::NewGuid().ToString())
RegularPath $directory
New-Item -ItemType Directory -Path $directory | Out-Null
$receipt = [ordered]@{schema='cs3-document-remediation-build/1';status='failed';exit_code=$null;source_inputs_unchanged=$false;toolchain_unchanged=$false;executable_sha256=$null;source_manifest=[ordered]@{path=$SourceManifestPath;sha256=$SourceManifestSha256};provider_calls=0;tests_executed=0;qualification_build=$true;production_release=$false;working_directory=$repository;target_directory=$target;builder_sha256=Hash $PSCommandPath;started_at=[DateTime]::UtcNow.ToString('o')}
$file = Join-Path $directory 'build-receipt.json'
$oldStack = $env:RUST_MIN_STACK
$before = $null
$toolsBefore = $null
Push-Location $repository
try {
    $env:RUST_MIN_STACK = '16777216'
    $before = Capture-Source
    if ($before -cne $sourceManifest) { throw 'Current source differs from the frozen source manifest' }
    $receipt.source_inputs = $before | ConvertFrom-Json
    $toolsBefore = Capture-Toolchain
    $receipt.toolchain = $toolsBefore | ConvertFrom-Json
    [IO.File]::WriteAllText((Join-Path $directory 'source-before.json'),$before,[Text.UTF8Encoding]::new($false))
    & $node scripts/upstream/reconstruct.cjs verify --component codex *> (Join-Path $directory 'upstream-before.log')
    if ($LASTEXITCODE -ne 0) { throw 'Frozen upstream verification failed before build' }
    $arguments = @('+1.98.0','test','--manifest-path',(Join-Path $workspace 'Cargo.toml'),'--locked','--offline','-p','vcp-cli','--features','qualification','--test','executable','--no-run','--target-dir',$target,'-j2','--message-format=json-render-diagnostics')
    $receipt.cargo_command = @($cargoLauncher) + $arguments
    & $cargoLauncher @arguments *> (Join-Path $directory 'build.log')
    $receipt.exit_code = $LASTEXITCODE
    & $node scripts/upstream/reconstruct.cjs verify --component codex *> (Join-Path $directory 'upstream-after.log')
    $upstreamAfter = $LASTEXITCODE
    if ($receipt.exit_code -ne 0 -or $upstreamAfter -ne 0) { throw 'Build or final upstream verification failed' }
    $built = Join-Path $target 'debug/vcp.exe'
    $compilerArtifacts = @(Get-Content -LiteralPath (Join-Path $directory 'build.log') | Where-Object { $_.StartsWith('{') } | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.reason -ceq 'compiler-artifact' -and $_.target.name -ceq 'vcp' -and $_.executable -and -not $_.profile.test })
    if ($compilerArtifacts.Count -ne 1 -or -not [IO.Path]::IsPathFullyQualified($compilerArtifacts[0].executable) -or [IO.Path]::GetFullPath($compilerArtifacts[0].executable) -ine $built -or @($compilerArtifacts[0].features).Count -ne 1 -or $compilerArtifacts[0].features[0] -cne 'qualification') { throw 'Exact qualification CLI compiler artifact unavailable' }
    $receipt.compiler_artifact = $compilerArtifacts[0]
    RegularPath $built
    $compiledHash = Hash $built
    $executable = Join-Path $directory 'vcp.exe'
    [IO.File]::Copy($built,$executable,$false)
    $receipt.executable = $executable
    $receipt.executable_sha256 = Hash $executable
    if ($receipt.executable_sha256 -cne $compiledHash -or (Hash $built) -cne $compiledHash) { throw 'Compiled executable changed during staging' }
    New-Item -ItemType Directory -Path (Join-Path $directory 'skills') | Out-Null
    $assetJson = & $node scripts/skills/builtin-assets.cjs stage (Join-Path $repository 'src/skills/builtin') (Join-Path $directory 'skills/builtin')
    if ($LASTEXITCODE -ne 0) { throw 'Builtin asset staging failed' }
    $receipt.builtin_inventory = $assetJson | ConvertFrom-Json
    & $node -e 'const fs=require("node:fs");require("./scripts/evals/builtin-generation-prepare.cjs").requireEmbeddedCatalog(fs.readFileSync(process.argv[1]),fs.readFileSync(process.argv[2]));' $executable (Join-Path $directory 'skills/builtin/catalog.json')
    if ($LASTEXITCODE -ne 0) { throw 'Embedded builtin catalog differs' }
    $receipt.inherited_cargo_configs = @()
    $receipt.workspace_config_applied = $false
    $receipt.limitations = @('Cached Cargo build with locked offline dependencies; not a clean-room reproducible build or production release.', 'Native tool hashes record installed compiler/linker candidates, not proof that a cached dependency was recompiled.', 'Source and toolchain are checked before/after, not locked against concurrent replacement.', 'The observed executable digest is prospective; it does not claim the historical qualification binary identity.')
    $receipt.upstream_before_sha256 = Hash (Join-Path $directory 'upstream-before.log')
    $receipt.upstream_after_sha256 = Hash (Join-Path $directory 'upstream-after.log')
} catch {
    $receipt.failure = $_.Exception.Message
} finally {
    # Final source/toolchain observations run even when Cargo or staging failed.
    # Failure cannot become a passed receipt through an expected-hash override.
    try {
        if ($null -ne $before) {
            $after = Capture-Source
            [IO.File]::WriteAllText((Join-Path $directory 'source-after.json'),$after,[Text.UTF8Encoding]::new($false))
            $receipt.source_inputs_unchanged = $before -ceq $after -and $after -ceq $sourceManifest -and (Hash $SourceManifestPath) -ceq $SourceManifestSha256
        }
        if ($null -ne $toolsBefore) {
            $toolsAfter = Capture-Toolchain
            $receipt.toolchain_unchanged = $toolsBefore -ceq $toolsAfter
        }
        if (@($configCandidates | Where-Object { Test-Path -LiteralPath $_ }).Count) { throw 'Inherited Cargo configuration changed during build' }
        if (-not $receipt.source_inputs_unchanged -or -not $receipt.toolchain_unchanged) { throw 'Source or toolchain changed during build' }
        if (-not $receipt.failure -and $receipt.exit_code -eq 0 -and $receipt.executable_sha256) { $receipt.status = 'passed' }
    } catch {
        $receipt.status = 'failed'
        $receipt.final_integrity_failure = $_.Exception.Message
    } finally {
        $env:RUST_MIN_STACK = $oldStack
        Pop-Location
    }
    $receipt.ended_at = [DateTime]::UtcNow.ToString('o')
    if (Test-Path -LiteralPath (Join-Path $directory 'build.log')) { $receipt.build_log_sha256 = Hash (Join-Path $directory 'build.log') }
    $receipt | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $file -Encoding utf8NoBOM
}
Write-Output $file
if ($receipt.status -cne 'passed') { exit 1 }
